#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec;
use core::mem;
use core::ptr::{copy_nonoverlapping, read_unaligned, write_bytes};
use uefi::prelude::*;
use uefi::proto::console::gop::{GraphicsOutput, PixelFormat};
use uefi::proto::media::file::{File, FileAttribute, FileInfo, FileMode, FileType};
use uefi::table::boot::{AllocateType, MemoryType};
use uefi::{cstr16, Status};

const KERNEL_PATH: &uefi::CStr16 = cstr16!("\\EFI\\PUREOS\\KERNEL.ELF");
const LOAD_GRANULARITY: usize = 4096;

fn serial_write(s: &str) {
    for &byte in s.as_bytes() {
        unsafe {
            core::arch::asm!("out dx, al", in("dx") 0x3F8u16, in("al") byte);
        }
    }
}

fn write_hex(val: u64) {
    let mut buf = [0u8; 18];
    buf[0] = b'0';
    buf[1] = b'x';
    for i in 0..16 {
        let nibble = ((val >> (60 - i * 4)) & 0xf) as u8;
        buf[2 + i] = if nibble < 10 { b'0' + nibble } else { b'a' + nibble - 10 };
    }
    // trim leading zeros
    let start = buf.iter().position(|&c| c != b'0').unwrap_or(16) - 1;
    serial_write(core::str::from_utf8(&buf[start..]).unwrap_or("?"));
}

fn write_dec(val: u64) {
    let mut buf = [0u8; 20];
    let mut n = val;
    let mut i = 19;
    loop {
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 || i == 0 {
            break;
        }
        i -= 1;
    }
    serial_write(core::str::from_utf8(&buf[i..]).unwrap_or("?"));
}

const ELF_MAGIC: &[u8; 4] = b"\x7fELF";
const ELF_CLASS_64: u8 = 2;
const ELF_DATA_LSB: u8 = 1;
const ELF_MACHINE_X86_64: u16 = 0x3e;
const PT_LOAD: u32 = 1;

#[repr(C)]
pub struct PureBootInfo {
    magic: u64,
    kernel_base: u64,
    kernel_size: u64,
    framebuffer_base: u64,
    framebuffer_size: u64,
    framebuffer_width: u32,
    framebuffer_height: u32,
    framebuffer_stride: u32,
    framebuffer_format: u32,
    // Пул физических фреймов, выданный загрузчиком (AllocatePages). Ядро строит
    // на нём свой frame-allocator. 0/0 — пул не удалось выделить.
    heap_base: u64,
    heap_size: u64,
    /// UEFI SystemTable — чтобы ядро могло вызывать UEFI Boot/Runtime Services
    system_table: u64,
    /// EFI_SIMPLE_TEXT_INPUT_PROTOCOL* — для клавиатуры без PS/2
    con_in: u64,
}

/// Размер пула физических фреймов для ядра: 64 MiB (16384 страницы по 4 KiB).
const FRAME_POOL_PAGES: usize = 16 * 1024;

#[entry]
fn efi_main(image: Handle, mut system_table: SystemTable<Boot>) -> Status {
    if uefi_services::init(&mut system_table).is_err() {
        return Status::ABORTED;
    }

    let boot_services = system_table.boot_services();

    let Some(mut graphics) = init_graphics(boot_services) else {
        return Status::UNSUPPORTED;
    };

    serial_write("[BOOT] Loading kernel...\n");
    let kernel = match load_kernel_image(image, boot_services) {
        Ok(kernel) => {
            serial_write("[BOOT] Kernel loaded OK\n");
            kernel
        }
        Err(status) => {
            serial_write("[BOOT] KERNEL LOAD FAILED\n");
            return status;
        }
    };

    let boot_info = build_boot_info(boot_services, &mut graphics, &kernel, &system_table);

    unsafe {
        boot_kernel(kernel.entry, boot_info);
    }
}

struct LoadedKernel {
    base: u64,
    entry: u64,
    size: usize,
}

#[repr(C)]
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

#[repr(C)]
#[derive(Copy, Clone)]
struct Elf64ProgramHeader {
    p_type: u32,
    flags: u32,
    offset: u64,
    vaddr: u64,
    paddr: u64,
    filesz: u64,
    memsz: u64,
    align: u64,
}

fn init_graphics(
    boot_services: &BootServices,
) -> Option<uefi::table::boot::ScopedProtocol<'_, GraphicsOutput>> {
    let handle = boot_services.get_handle_for_protocol::<GraphicsOutput>().ok()?;
    let mut gop = boot_services.open_protocol_exclusive::<GraphicsOutput>(handle).ok()?;

    if let Some(mode) = gop
        .modes(boot_services)
        .filter(|mode| mode.info().pixel_format() != PixelFormat::BltOnly)
        .max_by_key(|mode| {
            let (width, height) = mode.info().resolution();
            width * height
        })
    {
        let _ = gop.set_mode(&mode);
    }

    Some(gop)
}

fn load_kernel_image(image: Handle, boot_services: &BootServices) -> Result<LoadedKernel, Status> {
    let mut fs = boot_services
        .get_image_file_system(image)
        .map_err(|err| err.status())?;
    let mut root = fs.open_volume().map_err(|err| err.status())?;
    let handle = root
        .open(KERNEL_PATH, FileMode::Read, FileAttribute::empty())
        .map_err(|err| err.status())?;

    let mut file = match handle.into_type().map_err(|err| err.status())? {
        FileType::Regular(file) => file,
        FileType::Dir(_) => return Err(Status::LOAD_ERROR),
    };

    let info = file.get_boxed_info::<FileInfo>().map_err(|err| err.status())?;
    let size = info.file_size() as usize;
    if size == 0 {
        return Err(Status::LOAD_ERROR);
    }

    let mut buffer = vec![0u8; size];
    let read = file.read(&mut buffer).map_err(|err| err.status())?;
    if read != size {
        return Err(Status::LOAD_ERROR);
    }

    load_elf_segments(boot_services, &buffer)
}

fn load_elf_segments(boot_services: &BootServices, image: &[u8]) -> Result<LoadedKernel, Status> {
    if image.len() < mem::size_of::<Elf64Header>() {
        return Err(Status::LOAD_ERROR);
    }

    let header = unsafe { read_unaligned(image.as_ptr().cast::<Elf64Header>()) };
    if &header.ident[0..4] != ELF_MAGIC
        || header.ident[4] != ELF_CLASS_64
        || header.ident[5] != ELF_DATA_LSB
        || header.machine != ELF_MACHINE_X86_64
    {
        return Err(Status::LOAD_ERROR);
    }

    let phoff = header.phoff as usize;
    let phentsize = header.phentsize as usize;
    let phnum = header.phnum as usize;
    if phentsize < mem::size_of::<Elf64ProgramHeader>() {
        return Err(Status::LOAD_ERROR);
    }

    let mut lowest_base = u64::MAX;
    let mut highest_end = 0u64;

    for index in 0..phnum {
        let ph_offset = phoff + index * phentsize;
        if ph_offset + mem::size_of::<Elf64ProgramHeader>() > image.len() {
            return Err(Status::LOAD_ERROR);
        }

        let ph = unsafe {
            read_unaligned(image.as_ptr().add(ph_offset).cast::<Elf64ProgramHeader>())
        };
        if ph.p_type != PT_LOAD {
            continue;
        }
        if ph.filesz > ph.memsz {
            return Err(Status::LOAD_ERROR);
        }

        let segment_offset = ph.offset as usize;
        let file_size = ph.filesz as usize;
        let memory_size = ph.memsz as usize;
        if segment_offset + file_size > image.len() {
            return Err(Status::LOAD_ERROR);
        }

        let load_addr = if ph.paddr != 0 { ph.paddr } else { ph.vaddr };
        let page_base = load_addr & !((LOAD_GRANULARITY as u64) - 1);
        let page_offset = (load_addr - page_base) as usize;
        let pages = (page_offset + memory_size + LOAD_GRANULARITY - 1) / LOAD_GRANULARITY;

        serial_write("[BOOT]  Alloc ");
        write_hex(page_base);
        serial_write(" (");
        write_dec(pages as u64);
        serial_write(" pages)\n");

        boot_services
            .allocate_pages(
                AllocateType::Address(page_base),
                MemoryType::LOADER_DATA,
                pages,
            )
            .map_err(|err| {
                serial_write("[BOOT]  ALLOC FAILED!\n");
                err.status()
            })?;

        unsafe {
            let dest = load_addr as *mut u8;
            copy_nonoverlapping(image.as_ptr().add(segment_offset), dest, file_size);
            if memory_size > file_size {
                write_bytes(dest.add(file_size), 0, memory_size - file_size);
            }
        }

        lowest_base = lowest_base.min(page_base);
        highest_end = highest_end.max(page_base + (pages * LOAD_GRANULARITY) as u64);
    }

    if lowest_base == u64::MAX {
        return Err(Status::LOAD_ERROR);
    }

    Ok(LoadedKernel {
        base: lowest_base,
        entry: header.entry,
        size: (highest_end - lowest_base) as usize,
    })
}

fn build_boot_info(
    boot_services: &BootServices,
    gop: &mut GraphicsOutput,
    kernel: &LoadedKernel,
    system_table: &SystemTable<Boot>,
) -> *const PureBootInfo {
    let mode = gop.current_mode_info();
    let (width, height) = mode.resolution();
    let mut fb = gop.frame_buffer();
    let boot_info_ptr = boot_services
        .allocate_pool(MemoryType::LOADER_DATA, mem::size_of::<PureBootInfo>())
        .expect("boot info allocation failed")
        .cast::<PureBootInfo>();

    let pixel_format = match mode.pixel_format() {
        PixelFormat::Rgb => 1,
        PixelFormat::Bgr => 2,
        PixelFormat::Bitmask => 3,
        PixelFormat::BltOnly => 4,
    };

    // Зарезервировать у UEFI непрерывный пул физпамяти под frame-allocator ядра.
    // ExitBootServices не вызываем, поэтому регион остаётся за нами. При неудаче
    // передаём 0/0 — ядро деградирует до пустого пула (alloc → OOM), без падения.
    let (heap_base, heap_size) = match boot_services.allocate_pages(
        AllocateType::AnyPages,
        MemoryType::LOADER_DATA,
        FRAME_POOL_PAGES,
    ) {
        Ok(addr) => (addr, (FRAME_POOL_PAGES * LOAD_GRANULARITY) as u64),
        Err(_) => (0, 0),
    };

    let (con_in_val, st_val) = {
        let st = system_table.as_ptr();
        let st_ptr = st as *const u8;
        let con_in_ptr = unsafe { *(st_ptr.add(48) as *const u64) };
        (con_in_ptr, st as u64)
    };

    unsafe {
        boot_info_ptr.write(PureBootInfo {
            magic: 0x5055_5245_4f53_0001,
            kernel_base: kernel.base,
            kernel_size: kernel.size as u64,
            framebuffer_base: fb.as_mut_ptr() as u64,
            framebuffer_size: fb.size() as u64,
            framebuffer_width: width as u32,
            framebuffer_height: height as u32,
            framebuffer_stride: mode.stride() as u32,
            framebuffer_format: pixel_format,
            heap_base,
            heap_size,
            system_table: st_val,
            con_in: con_in_val,
        });

        serial_write("[BOOT] boot-info ptr=");
        write_hex(boot_info_ptr as u64);
        serial_write(" magic=");
        write_hex(boot_info_ptr.read().magic);
        serial_write("\n");
    }

    boot_info_ptr.cast_const()
}

unsafe fn boot_kernel(entry: u64, boot_info: *const PureBootInfo) -> ! {
    let entry_fn: extern "C" fn(*const PureBootInfo) -> ! = core::mem::transmute(entry);
    entry_fn(boot_info);
}
