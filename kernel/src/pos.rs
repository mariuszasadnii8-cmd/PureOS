//! PureOS executable (.pos) loader.
//!
//! .pos — минимальный формат исполняемых файлов PureOS.
//! Представляет собой контейнер для сырого x86_64 машкода,
//! который оборачивается в минимальный ELF64 и запускается
//! как ring3-процесс через `elf::exec`.
//!
//! Формат файла:
//!   [0..4]   magic     "POS\0"
//!   [4]      version   0x01
//!   [5]      flags     (bit 0 = 1: есть data-секция)
//!   [6..7]   reserved  (0)
//!   [8..16]  entry     u64, смещение точки входа от начала файла
//!   [16..]   code      x86_64 машкод (position-independent)
//!
//! Код может использовать syscall (инструкция `syscall`).
//! Для вывода чисел: syscall 32 (print_num) — rdi=value, rsi≠0 → \n.

use crate::elf;
use crate::syscall;

const POS_MAGIC: [u8; 4] = [b'P', b'O', b'S', 0];
const POS_HEADER_SIZE: usize = 16;

/// Загрузить и выполнить .pos-файл из буфера в памяти.
/// Возвращает PID процесса или отрицательный код ошибки.
pub unsafe fn exec(data: &[u8]) -> i64 {
    if data.len() < POS_HEADER_SIZE {
        return syscall::ERR_INVALID_POINTER;
    }

    // Проверить magic
    if data[0..4] != POS_MAGIC {
        return syscall::ERR_INVALID_POINTER;
    }

    let _version = data[4];
    let _flags = data[5];
    let entry = u64::from_le_bytes([
        data[8], data[9], data[10], data[11],
        data[12], data[13], data[14], data[15],
    ]);

    let code_start = POS_HEADER_SIZE;
    if entry < code_start as u64 || entry as usize > data.len() || code_start >= data.len() {
        return syscall::ERR_INVALID_POINTER;
    }

    let code_len = data.len() - code_start;

    // Создать минимальный ELF64, оборачивающий код.
    // Используем статический буфер, как barrelc.
    let mut elf_buf = [0u8; 4096];
    let total = build_minimal_elf(&mut elf_buf, &data[code_start..], entry - code_start as u64);
    if total == 0 {
        return syscall::ERR_OUT_OF_MEMORY;
    }

    elf::exec(core::ptr::addr_of!(elf_buf) as u64, total as u64)
}

/// Построить минимальный ELF64 в buf из сырого машкода.
/// Возвращает общий размер ELF или 0 при ошибке.
unsafe fn build_minimal_elf(buf: &mut [u8; 4096], code: &[u8], entry_offset: u64) -> usize {
    let phdr_count = 2u16; // code segment + stack (GNU_STACK)
    let ehsize = 64u16;
    let phentsize = 56u16;
    let phoff = ehsize as u64;
    let code_vaddr = 0x400000u64;
    let stack_vaddr = 0x7ffff000u64;
    let code_size = code.len().next_power_of_two().max(4096);
    let stack_size = 0x10000u64; // 64 KiB

    let total = (ehsize + phentsize * phdr_count) as usize;
    if total + code.len() > buf.len() {
        return 0;
    }

    // ELF header
    buf[0..4].copy_from_slice(&[0x7f, b'E', b'L', b'F']); // magic
    buf[4] = 2;  // ELFCLASS64
    buf[5] = 1;  // ELFDATA2LSB
    buf[6] = 1;  // EV_CURRENT
    // ident[7..16] = 0
    buf[16..18].copy_from_slice(&(2u16).to_le_bytes()); // ET_EXEC
    buf[18..20].copy_from_slice(&(0x3e_u16).to_le_bytes()); // EM_X86_64
    buf[20..24].copy_from_slice(&(1u32).to_le_bytes()); // version
    buf[24..32].copy_from_slice(&(code_vaddr + entry_offset).to_le_bytes()); // entry
    buf[32..40].copy_from_slice(&phoff.to_le_bytes()); // phoff
    buf[40..48].copy_from_slice(&(0u64).to_le_bytes()); // shoff
    buf[48..52].copy_from_slice(&(0u32).to_le_bytes()); // flags
    buf[52..54].copy_from_slice(&ehsize.to_le_bytes());
    buf[54..56].copy_from_slice(&phentsize.to_le_bytes());
    buf[56..58].copy_from_slice(&phdr_count.to_le_bytes()); // phnum
    buf[58..60].copy_from_slice(&(0u16).to_le_bytes()); // shentsize
    buf[60..62].copy_from_slice(&(0u16).to_le_bytes()); // shnum
    buf[62..64].copy_from_slice(&(0u16).to_le_bytes()); // shstrndx

    // PHDR 1: LOAD (code)
    let ph1 = ehsize as usize;
    buf[ph1..ph1 + 4].copy_from_slice(&(1u32).to_le_bytes()); // PT_LOAD
    buf[ph1 + 4..ph1 + 8].copy_from_slice(&(5u32).to_le_bytes()); // p_flags: R+X
    buf[ph1 + 8..ph1 + 16].copy_from_slice(&(0u64).to_le_bytes()); // p_offset (in file)
    buf[ph1 + 16..ph1 + 24].copy_from_slice(&code_vaddr.to_le_bytes()); // p_vaddr
    buf[ph1 + 24..ph1 + 32].copy_from_slice(&code_vaddr.to_le_bytes()); // p_paddr
    buf[ph1 + 32..ph1 + 40].copy_from_slice(&(code.len() as u64).to_le_bytes()); // p_filesz
    buf[ph1 + 40..ph1 + 48].copy_from_slice(&(code_size as u64).to_le_bytes()); // p_memsz
    buf[ph1 + 48..ph1 + 56].copy_from_slice(&(0x1000u64).to_le_bytes()); // p_align

    // PHDR 2: GNU_STACK (for stack permissions)
    let ph2 = ph1 + 56;
    buf[ph2..ph2 + 4].copy_from_slice(&(0x6474e551u32).to_le_bytes()); // PT_GNU_STACK
    buf[ph2 + 4..ph2 + 8].copy_from_slice(&(6u32).to_le_bytes()); // p_flags: R+W
    buf[ph2 + 8..ph2 + 56].fill(0);
    buf[ph2 + 40..ph2 + 48].copy_from_slice(&stack_size.to_le_bytes()); // p_memsz

    // Код — сразу после PHDR
    let code_off = total;
    buf[code_off..code_off + code.len()].copy_from_slice(code);

    code_off + code.len()
}
