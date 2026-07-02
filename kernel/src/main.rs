#![no_std]
#![no_main]

use core::arch::naked_asm;
use core::panic::PanicInfo;
use core::ptr::addr_of_mut;

mod barrel;
mod barrelc;
mod commands;
mod config;
mod console;
mod context;
mod cpu;
mod documentation;
mod elf;
mod ephemeral;
mod font;
mod frame;
mod framebuffer;
mod graphics;
mod idt;
mod installer;
mod keyboard;
mod shell;
mod syscall;
mod terminal;
mod uefi;

/// Кристаллическая структура топологии ядра (замораживается при старте).
pub struct CrystalTopology {
    pub cpu_count: usize,
    pub ram_base: u64,
    pub ram_size: u64,
    pub rom_base: u64,
    pub rom_size: u64,
}

#[repr(C)]
pub struct PureBootInfo {
    pub magic: u64,
    pub kernel_base: u64,
    pub kernel_size: u64,
    pub framebuffer_base: u64,
    pub framebuffer_size: u64,
    pub framebuffer_width: u32,
    pub framebuffer_height: u32,
    pub framebuffer_stride: u32,
    pub framebuffer_format: u32,
    pub heap_base: u64,
    pub heap_size: u64,
    /// UEFI SystemTable
    pub system_table: u64,
    /// EFI_SIMPLE_TEXT_INPUT_PROTOCOL*
    pub con_in: u64,
}

const BOOT_MAGIC: u64 = 0x5055_5245_4f53_0001;

static mut TOPOLOGY: CrystalTopology = CrystalTopology {
    cpu_count: 0,
    ram_base: 0,
    ram_size: 0,
    rom_base: 0,
    rom_size: 0,
};

#[no_mangle]
pub extern "win64" fn _start(boot_info: *const PureBootInfo) -> ! {
    kernel_main(boot_info)
}

#[no_mangle]
pub extern "win64" fn kernel_main(boot_info: *const PureBootInfo) -> ! {
    unsafe {
        // КРИТИЧНО: UEFI оставляет прерывания включёнными и держит СВОЮ IDT,
        // чьи гейты ссылаются на UEFI-GDT. Как только мы ниже подменим GDT
        // (`init_gdt`), ближайший тик таймера UEFI вызовет #GP через устаревший
        // гейт -> тройная ошибка -> ребут. Поэтому глушим прерывания сразу и не
        // включаем их снова: планировщик кооперативный, клавиатура опрашивается.
        core::arch::asm!("cli", options(nomem, nostack));
        mask_legacy_pic();

        freeze_topology(boot_info);
        init_frame_pool(boot_info);

        // Инициализация фреймбуфера (GOP от загрузчика) — для boot-экрана
        let info = &*boot_info;
        console::serial_puts(b"[KERNEL] boot-info ptr=0x");
        console::serial_hex(boot_info as u64);
        console::serial_puts(b" first=0x");
        console::serial_hex(core::ptr::read_unaligned(boot_info as *const u64));
        console::serial_puts(b" second=0x");
        console::serial_hex(core::ptr::read_unaligned((boot_info as *const u8).add(8) as *const u64));
        console::serial_puts(b"\n");
        console::serial_puts(b"[KERNEL] boot-info magic=0x");
        console::serial_hex(info.magic);
        console::serial_puts(b" fb=0x");
        console::serial_hex(info.framebuffer_base);
        console::serial_puts(b" size=");
        console::serial_dec(info.framebuffer_size);
        console::serial_puts(b" res=");
        console::serial_dec(info.framebuffer_width as u64);
        console::serial_puts(b"x");
        console::serial_dec(info.framebuffer_height as u64);
        console::serial_puts(b"\n");

        if info.magic == BOOT_MAGIC {
            framebuffer::init(
                info.framebuffer_base,
                info.framebuffer_width,
                info.framebuffer_height,
                info.framebuffer_stride,
                info.framebuffer_format,
            );
            console::serial_puts(b"[KERNEL] framebuffer init ok\n");
        } else {
            console::serial_puts(b"[KERNEL] boot-info magic mismatch\n");
        }

        // Экранный текстовый терминал (рендерит глифы прямо в GOP-фреймбуфер).
        // Инициализируем сразу после фреймбуфера, чтобы дальнейший boot-лог
        // попадал прямо в консоль без промежуточного сплэш-экрана.
        terminal::init();

        // UEFI SystemTable + ConIn (нужно для клавиатуры и reboot; НЕ для вывода)
        uefi::init(info.system_table, info.con_in);
        console::boot_msg(b"[UEFI] SystemTable & protocols initialized\n");
        console::boot_msg(b"[BOOT] Crystal topology frozen\n");

        // CPU: GDT/TSS
        console::boot_msg(b"[CPU] Init GDT/TSS...\n");
        cpu::init_gdt();
        console::boot_msg(b"[CPU] OK\n");

        // IDT — свои обработчики исключений (иначе любой #PF/#GP = тройная ошибка).
        console::boot_msg(b"[IDT] Init interrupt descriptor table...\n");
        idt::init();
        idt::load();
        console::boot_msg(b"[IDT] OK\n");

        // Process manager
        console::boot_msg(b"[SYS] Init process table...\n");
        syscall::init_process_manager();
        console::boot_msg(b"[SYS] OK\n");

        // Syscall MSRs
        console::boot_msg(b"[SYS] Init SYSCALL/SYSRET...\n");
        syscall::init_syscall_msrs();
        console::boot_msg(b"[SYS] OK\n");

        // Клавиатура (UEFI Simple Text Input)
        console::boot_msg(b"[KBD] Init UEFI keyboard...\n");
        keyboard::init();
        console::boot_msg(b"[KBD] OK\n");

        // Показать аккуратный boot-баннер и передать управление оболочке,
        // чтобы QEMU отображал живой консольный промпт, а не статичную заглушку.
        console::boot_msg(b"[SYS] Console ready. Entering shell.\n");
        terminal::draw_boot_banner();
        shell::run();
    }
}

/// Замаскировать все линии legacy-PIC (8259A). Подстраховка на случай, если
/// платформа маршрутизирует таймер/устройства через PIC: даже при случайном
/// `sti` в будущем незапрошенные IRQ не полетят. LAPIC-таймер это не трогает,
/// но у нас и без того `cli` держится весь рантайм.
unsafe fn mask_legacy_pic() {
    cpu::outb(0x21, 0xFF); // master PIC: маскировать IRQ0..7
    cpu::outb(0xA1, 0xFF); // slave PIC:  маскировать IRQ8..15
}

unsafe fn freeze_topology(boot_info: *const PureBootInfo) {
    if boot_info.is_null() { return; }
    let info = &*boot_info;
    if info.magic != BOOT_MAGIC { return; }

    let topo = addr_of_mut!(TOPOLOGY);
    (*topo).cpu_count = 1;
    (*topo).ram_base = info.heap_base;
    (*topo).ram_size = info.heap_size;
    (*topo).rom_base = 0;
    (*topo).rom_size = 0;
}

unsafe fn init_frame_pool(boot_info: *const PureBootInfo) {
    if boot_info.is_null() { return; }
    let info = &*boot_info;
    if info.magic != BOOT_MAGIC { return; }
    frame::init(info.heap_base, info.heap_size);
}

#[unsafe(naked)]
pub(crate) unsafe extern "C" fn user_demo() -> ! {
    naked_asm!(
        "2:",
        "mov rax, 5",
        "xor rdi, rdi",
        "xor rsi, rsi",
        "xor rdx, rdx",
        "syscall",
        "jmp 2b",
    );
}

#[inline(always)]
fn arch_hlt() {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        arch_hlt();
    }
}
