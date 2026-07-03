//! Минимальный ELF64-загрузчик для статически слинкованных PIE-бинарников.
//!
//! Загружает сегменты PT_LOAD в эфемерный слой процесса и возвращает entry point.
//! ELF обязан быть статическим (без DT_NEEDED), relocs не применяются — бинарник
//! компилируется как static PIE (по умолчанию у Rust/Go/Zig для x86_64).
//!
//! Это и есть «FFI-мост» для внешних языков: любой язык, умеющий компилировать
//! статический x86_64 ELF, может работать как userspace-программа PureOS.

use core::mem;
use core::ptr::read_volatile;

use crate::cpu;
use crate::frame;
use crate::syscall;

/// Номер syscall для exec_elf (документирующая константа для юзерленда).
#[allow(dead_code)]
pub const SYS_EXEC_ELF: u64 = 15;

const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];
const PT_LOAD: u32 = 1;

#[repr(C, packed)]
#[derive(Copy, Clone)]
struct Elf64Header {
    ident: [u8; 16],
    file_type: u16,
    machine: u16,
    version: u32,
    entry: u64,
    phoff: u64,
    shoff: u64,
    flags: u32,
    ehsize: u16,
    phentsize: u16,
    phnum: u16,
    shentsize: u16,
    shnum: u16,
    shstrndx: u16,
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
struct Elf64Phdr {
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_paddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
}

/// Главная точка входа: загрузить ELF как новый процесс.
pub unsafe fn exec(data_ptr: u64, size: u64) -> i64 {
    if size < mem::size_of::<Elf64Header>() as u64 {
        return syscall::ERR_INVALID_POINTER;
    }

    let hdr: Elf64Header = read_val(data_ptr);

    if hdr.ident[0..4] != ELF_MAGIC
        || hdr.ident[4] != 2       // ELFCLASS64
        || hdr.machine != 0x3e     // EM_X86_64
    {
        return syscall::ERR_INVALID_POINTER;
    }
    if hdr.phnum == 0 || (hdr.phentsize as usize) < mem::size_of::<Elf64Phdr>() {
        return syscall::ERR_INVALID_POINTER;
    }

    let phoff = hdr.phoff as usize;
    let phnum = hdr.phnum as usize;
    let phentsize = hdr.phentsize as usize;

    // Первый проход: найти диапазон адресов под все PT_LOAD.
    let mut range = SegmentRange { base: u64::MAX, end: 0 };
    for i in 0..phnum {
        let ph: Elf64Phdr = read_val(data_ptr + phoff as u64 + (i * phentsize) as u64);
        if ph.p_type != PT_LOAD {
            continue;
        }
        if ph.p_filesz > ph.p_memsz {
            return syscall::ERR_INVALID_POINTER;
        }
        let ok = range.extend(ph.p_vaddr, ph.p_memsz);
        if !ok {
            return syscall::ERR_INVALID_POINTER;
        }
    }
    if range.base == u64::MAX {
        return syscall::ERR_INVALID_POINTER;
    }

    let load_base = range.base & !0xFFF;
    let load_end = (range.end + 0xFFF) & !0xFFF;

    // Найти слот процесса.
    let slot = match syscall::find_free_slot() {
        Some(s) => s,
        None => return syscall::ERR_NO_CAPACITY,
    };
    syscall::clone_current_pml4_for(slot);
    let pml4 = syscall::process_pml4_phys(slot);

    // Инициализировать список фреймов (frame tracking для free на exit).
    syscall::PROCESS_TABLE[slot].frames_head = 0;
    let pid = slot;

    // Второй проход: отобразить страницы и скопировать данные сегментов.
    for i in 0..phnum {
        let ph: Elf64Phdr = read_val(data_ptr + phoff as u64 + (i * phentsize) as u64);
        if ph.p_type != PT_LOAD {
            continue;
        }

        let file_data = ph.p_offset;
        let seg_start = ph.p_vaddr;
        let seg_filesz = ph.p_filesz;
        let seg_memsz = ph.p_memsz;
        let writable = (ph.p_flags & 2) != 0;

        let page_start = seg_start & !0xFFF;
        let page_end = (seg_start + seg_memsz + 0xFFF) & !0xFFF;

        let mut va = page_start;
        while va < page_end {
            let phys = match frame::alloc_frame() {
                Some(p) => p,
                None => return syscall::ERR_OUT_OF_MEMORY,
            };
            syscall::track_frame(pid, phys);

            if !cpu::map_page(pml4, va, phys, true, writable) {
                return syscall::ERR_OUT_OF_MEMORY;
            }
            cpu::invlpg(va);

            // Сколько байт копировать из ELF в эту страницу.
            let in_page = va - seg_start;
            let file_avail = seg_filesz.saturating_sub(in_page);
            let copy_now = 0x1000.min(file_avail as usize);
            if copy_now > 0 {
                copy_from_user(data_ptr + file_data + in_page as u64, phys, copy_now);
            }
            // Остаток страницы (bss) уже занулён alloc_frame.

            va += 0x1000;
        }
    }

    // Стек пользователя — следом за кодом.
    let stack_size: u64 = 64 * 1024;
    let stack_base = load_end;
    let stack_top = stack_base + stack_size;
    let stack_pages = stack_size as usize / 0x1000;

    for p in 0..stack_pages {
        let va = stack_base + p as u64 * 0x1000;
        let phys = match frame::alloc_frame() {
            Some(ph) => ph,
            None => return syscall::ERR_OUT_OF_MEMORY,
        };
        syscall::track_frame(pid, phys);
        if !cpu::map_page(pml4, va, phys, true, true) {
            return syscall::ERR_OUT_OF_MEMORY;
        }
        cpu::invlpg(va);
    }

    let entry = hdr.entry;
    syscall::provision_pcb_elf(
        slot,
        entry,
        pml4,
        stack_top,
        load_base,
        load_end - load_base,
    );

    slot as i64
}

// ---------------------------------------------------------------------------
// Внутренние утилиты
// ---------------------------------------------------------------------------

struct SegmentRange {
    base: u64,
    end: u64,
}

impl SegmentRange {
    fn extend(&mut self, vaddr: u64, memsz: u64) -> bool {
        let end = match vaddr.checked_add(memsz) {
            Some(e) => e,
            None => return false,
        };
        self.base = self.base.min(vaddr);
        self.end = self.end.max(end);
        true
    }
}

/// Прочитать значение T из `addr` (данные могут быть в userspace).
unsafe fn read_val<T: Copy>(addr: u64) -> T {
    let mut val: T = mem::zeroed();
    let src = addr as *const u8;
    let dst = &mut val as *mut T as *mut u8;
    for i in 0..mem::size_of::<T>() {
        dst.add(i).write(read_volatile(src.add(i)));
    }
    val
}

/// Побайтово скопировать из userspace `src` в физический адрес `dst`.
unsafe fn copy_from_user(src: u64, dst: u64, len: usize) {
    for i in 0..len {
        (dst as *mut u8).add(i).write(read_volatile((src + i as u64) as *const u8));
    }
}
