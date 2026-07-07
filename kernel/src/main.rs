#![no_std]
#![no_main]

use core::arch::naked_asm;
use core::panic::PanicInfo;
use core::ptr::addr_of_mut;

mod acpi;
mod apic;
mod ata;
mod barrel;
mod barrelc;
mod blockfs;
mod commands;
mod config;
mod pos;
mod console;
mod context;
mod cpu;
mod desktop;
mod documentation;
mod elf;
mod ephemeral;
mod filer;
mod font;
mod fs;
mod image;
mod frame;
mod framebuffer;
mod graphics;
mod hw;
mod idt;
mod installer;
mod keyboard;
mod ps2mouse;
mod settings;
mod shell;
mod smp;
mod snake_game;
mod syscall;
mod sysmon;
mod terminal;
mod test_runner;
mod uefi;
mod wallpaper;
mod window;
mod cmos;
mod pcspeaker;
mod sound;
mod math;
mod fun;
mod gfx3d;
mod jpeg;
mod gif;
mod net;
mod paint;
mod usb;

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
    /// Общий объём доступной оперативной памяти (по UEFI memory map).
    pub total_ram: u64,
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
        fs::init();

        // Показываем прогресс-бар если есть фреймбуфер
        let info = &*boot_info;
        if info.magic == BOOT_MAGIC {
            framebuffer::init(
                info.framebuffer_base,
                info.framebuffer_width,
                info.framebuffer_height,
                info.framebuffer_stride,
                info.framebuffer_format,
            );
        }

        // Передаём в hw размер пула фреймов как ориентир доступной RAM.
        let (ram_b, ram_s) = if !boot_info.is_null() && (*boot_info).magic == BOOT_MAGIC {
            ((*boot_info).heap_base, (*boot_info).heap_size)
        } else {
            (0, 0)
        };
        hw::init(ram_b, ram_s);

        // Прогресс: 5%
        console::boot_progress(5);

        // Инициализация ATA-диска и блочной ФС (персистентное хранилище).
        console::boot_msg(b"[ATA] Init disk...\n");
        blockfs::init();
        console::boot_msg(b"[ATA] OK\n");
        console::boot_progress(10);

        // Экранный текстовый терминал (рендерит глифы прямо в GOP-фреймбуфер).
        terminal::init();

        // Показать начальный boot-прогресс
        console::boot_progress(12);

        // UEFI SystemTable + ConIn
        uefi::init(info.system_table, info.con_in);
        console::boot_msg(b"[UEFI] SystemTable & protocols initialized\n");
        console::boot_msg(b"[BOOT] Crystal topology frozen\n");
        console::boot_progress(15);

        // CPU: GDT/TSS
        console::boot_msg(b"[CPU] Init GDT/TSS...\n");
        cpu::init_gdt();
        console::boot_msg(b"[CPU] OK\n");
        console::boot_progress(20);

        // IDT
        console::boot_msg(b"[IDT] Init interrupt descriptor table...\n");
        idt::init();
        idt::load();
        console::boot_msg(b"[IDT] OK\n");
        console::boot_progress(25);

        // APIC: детекция x2APIC и калибровка таймера
        console::boot_msg(b"[APIC] Detecting x2APIC mode...\n");
        apic::detect_x2apic();
        console::boot_msg(b"[APIC] Calibrating timer...\n");
        apic::calibrate();
        console::boot_msg(b"[APIC] Timer disabled (cooperative scheduler)\n");
        console::boot_progress(30);

        // Process manager
        console::boot_msg(b"[SYS] Init process table...\n");
        syscall::init_process_manager();
        console::boot_msg(b"[SYS] OK\n");
        console::boot_progress(35);

        // Syscall MSRs
        console::boot_msg(b"[SYS] Init SYSCALL/SYSRET...\n");
        syscall::init_syscall_msrs();
        console::boot_msg(b"[SYS] OK\n");
        console::boot_progress(40);

        // Клавиатура
        console::boot_msg(b"[KBD] Init UEFI keyboard...\n");
        keyboard::init();
        console::boot_msg(b"[KBD] OK\n");
        console::boot_progress(45);

        // SMP
        console::boot_msg(b"[SMP] Init symmetric multiprocessing...\n");
        smp::init();
        console::boot_msg(b"[SMP] OK\n");
        console::boot_progress(60);

        // USB
        console::boot_msg(b"[USB] Init USB subsystem...\n");
        usb::init();
        console::boot_msg(b"[USB] OK\n");
        console::boot_progress(70);

        // Первый опрос USB
        console::boot_msg(b"[USB] Polling for devices...\n");
        usb::poll();
        console::boot_progress(80);

        // Draw boot banner
        console::boot_msg(b"[SYS] Console ready. Entering shell.\n");
        console::boot_progress(100);
        terminal::draw_boot_banner();

        // Boot jingle
        console::boot_msg(b"[SND] Initializing sound...\n");
        sound::boot();
        console::boot_msg(b"[SND] OK\n");

        // RTC init (read once to warm up)
        let _rtc = cmos::read_rtc();
        console::boot_msg(b"[RTC] Real-time clock active\n");

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
    let n = crate::hw::cpu_threads() as usize;
    (*topo).cpu_count = if n > 0 { n } else { 1 };
    (*topo).ram_base = info.heap_base;
    (*topo).ram_size = info.total_ram.max(info.heap_size);
    (*topo).rom_base = 0;
    (*topo).rom_size = 0;

    // Сохранить total_ram в frame-allocator для статистики
    frame::set_total_physical_memory(info.total_ram);
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
