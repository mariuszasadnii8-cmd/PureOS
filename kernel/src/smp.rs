//! SMP — symmetric multiprocessing.
//!
//! AP wakeup via INIT-SIPI-SIPI protocol.
//! Trampoline at physical 0x8000 transitions APs from real mode → long mode.
//! Per-CPU data, work queue, and IPI dispatch.
//!
//! Zero-Alloc: all structures are static arrays of fixed size.

use core::arch::{asm, naked_asm};
use core::ptr::{addr_of, write_volatile, addr_of_mut};

use crate::apic;
use crate::cpu;
use crate::terminal;

// ---------------------------------------------------------------------------
// Конфигурация SMP
// ---------------------------------------------------------------------------

/// Максимальное число логических процессоров.
pub const MAX_CPUS: usize = 8;

/// Физический адрес страницы с информацией для AP (cr3, percpu, ap_entry).
const TRAMPOLINE_INFO_PHYS: u64 = 0x7000;

/// Физический адрес страницы с кодом трамплина (SIPI vector 7).
const TRAMPOLINE_CODE_PHYS: u64 = 0x8000;

/// SIPI vector для wakeup AP (адрес = vector * 0x1000 = 0x8000).
const SIPI_VECTOR: u8 = 0x08;

/// Размер стека ядра на каждый AP.
const AP_STACK_SIZE: usize = 16384;

/// Максимум элементов в очереди работы (IPI).
const WORK_QUEUE_SIZE: usize = 32;

// ---------------------------------------------------------------------------
// Per-CPU структура (расширенная версия из cpu.rs)
// ---------------------------------------------------------------------------

/// Per-CPU блок, адресуемый через GS после `swapgs`.
/// Первые два поля (kernel_rsp, user_rsp_scratch) совпадают с cpu::PerCpu
/// и используются трамплином syscall_entry.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PerCpu {
    pub kernel_rsp: u64,       // gs:[0]
    pub user_rsp_scratch: u64, // gs:[8]
    pub cpu_id: u32,           // gs:[16] — логический номер CPU
    pub apic_id: u32,           // gs:[20] — APIC ID
    pub stack_base: u64,        // gs:[24] — база стека ядра
    pub ap_ready: u32,          // gs:[32] — 1 если AP инициализирован
}

/// Массив Per-CPU структур для всех процессоров.
pub static mut PERCPU_ARRAY: [PerCpu; MAX_CPUS] = [PerCpu {
    kernel_rsp: 0,
    user_rsp_scratch: 0,
    cpu_id: 0,
    apic_id: 0,
    stack_base: 0,
    ap_ready: 0,
}; MAX_CPUS];

/// Счётчик обнаруженных процессоров (BSP + AP).
static mut CPU_COUNT: usize = 1;

/// Счётчик AP, завершивших инициализацию (для синхронизации).
static mut AP_READY_COUNT: usize = 0;

// ---------------------------------------------------------------------------
// Стеки для AP
// ---------------------------------------------------------------------------

/// Стеки ядра для AP (по 16 КБ на каждый).
static mut AP_STACKS: [u8; MAX_CPUS * AP_STACK_SIZE] = [0; MAX_CPUS * AP_STACK_SIZE];

// ---------------------------------------------------------------------------
// Очередь работы (IPI dispatch)
// ---------------------------------------------------------------------------

/// Тип функции-обработчика работы по IPI.
type WorkFn = unsafe fn(arg: u64);

#[derive(Clone, Copy)]
struct WorkItem {
    handler: Option<WorkFn>,
    arg: u64,
}

/// Per-CPU очередь работы (каждый CPU обрабатывает свои).
static mut WORK_QUEUES: [[WorkItem; WORK_QUEUE_SIZE]; MAX_CPUS] = [[WorkItem {
    handler: None,
    arg: 0,
}; WORK_QUEUE_SIZE]; MAX_CPUS];

/// Индексы записи для каждой очереди.
static mut WORK_QUEUE_HEADS: [usize; MAX_CPUS] = [0; MAX_CPUS];
static mut WORK_QUEUE_TAILS: [usize; MAX_CPUS] = [0; MAX_CPUS];

// ---------------------------------------------------------------------------
// Трамплин (real mode → long mode) — написан на ассемблере.
// ---------------------------------------------------------------------------

/// Трамплин для AP: переход 16-bit real → 32-bit protected → 64-bit long.
/// Реализован как ручной машинный код для точного контроля размещения.
/// Копируется на физический адрес 0x8000 при SMP init.
///
/// Разметка на странице 0x8000:
///   0x000: 16-bit code (≈32 байта)
///   0x020: GDT: null | kcode64 | kdata32 (24 байта)
///   0x038: GDTP 16 (6 байт: limit=23, base=0x8020)
///   0x03E: GDTP 64 (10 байт: limit=23, base=0x8020)
///   0x048: 32-bit code (≈56 байт)
///   0x080: 64-bit code
///
/// Инфоблок на 0x7000:
///   0x7000: u64 cr3
///   0x7008: u64 percpu_array_base (unused by trampoline, uses table at 0x7020)
///   0x7010: u64 ap_main entry
///   0x7020: u64[MAX_CPUS] percpu_ptr_table
const TRAMPOLINE: [u8; 512] = {
    // ======== 16-bit real mode code (0x000–0x01F) ========
    let mut t = [0u8; 512];

    // cli (FA)
    t[0] = 0xFA;
    // cld (FC)
    t[1] = 0xFC;
    // xor ax, ax (31 C0)
    t[2] = 0x31; t[3] = 0xC0;
    // mov ds, ax (8E D8)
    t[4] = 0x8E; t[5] = 0xD8;
    // mov es, ax (8E C0)
    t[6] = 0x8E; t[7] = 0xC0;
    // mov ss, ax (8E D0)
    t[8] = 0x8E; t[9] = 0xD0;
    // lgdt [0x8038] — 0F 01 15 <4-byte-addr>
    t[10] = 0x0F; t[11] = 0x01; t[12] = 0x15;
    t[13] = 0x38; t[14] = 0x80; t[15] = 0x00; t[16] = 0x00;
    // mov eax, cr0 (0F 20 C0)
    t[17] = 0x0F; t[18] = 0x20; t[19] = 0xC0;
    // or al, 1 (0C 01)
    t[20] = 0x0C; t[21] = 0x01;
    // mov cr0, eax (0F 22 C0)
    t[22] = 0x0F; t[23] = 0x22; t[24] = 0xC0;
    // Far jump: 66 EA <4B-offset> <2B-selector>
    t[25] = 0x66; t[26] = 0xEA;
    t[27] = 0x48; t[28] = 0x80; t[29] = 0x00; t[30] = 0x00;
    t[31] = 0x08; t[32] = 0x00;

    // ======== GDT data (0x020–0x047) ========
    // null descriptor (8 bytes)
    t[0x20] = 0x00; t[0x21] = 0x00; t[0x22] = 0x00; t[0x23] = 0x00;
    t[0x24] = 0x00; t[0x25] = 0x00; t[0x26] = 0x00; t[0x27] = 0x00;
    // kcode64: 00AF9A000000FFFF
    t[0x28] = 0xFF; t[0x29] = 0xFF; t[0x2A] = 0x00; t[0x2B] = 0x00;
    t[0x2C] = 0x00; t[0x2D] = 0x9A; t[0x2E] = 0xAF; t[0x2F] = 0x00;
    // kdata32: 00CF92000000FFFF
    t[0x30] = 0xFF; t[0x31] = 0xFF; t[0x32] = 0x00; t[0x33] = 0x00;
    t[0x34] = 0x00; t[0x35] = 0x92; t[0x36] = 0xCF; t[0x37] = 0x00;
    // GDTP 16: limit=23, base=0x8020 (6 bytes)
    t[0x38] = 0x17; t[0x39] = 0x00; // limit = 23
    t[0x3A] = 0x20; t[0x3B] = 0x80; t[0x3C] = 0x00; t[0x3D] = 0x00;
    // GDTP 64: limit=23, base=0x8020 (10 bytes)
    t[0x3E] = 0x17; t[0x3F] = 0x00;
    t[0x40] = 0x20; t[0x41] = 0x80; t[0x42] = 0x00; t[0x43] = 0x00;
    t[0x44] = 0x00; t[0x45] = 0x00; t[0x46] = 0x00; t[0x47] = 0x00;

    // ======== 32-bit protected mode code (0x048–0x07F) ========
    // mov ax, 0x10 (66 B8 10 00)
    t[0x48] = 0x66; t[0x49] = 0xB8; t[0x4A] = 0x10; t[0x4B] = 0x00;
    // mov ds, ax (8E D8)
    t[0x4C] = 0x8E; t[0x4D] = 0xD8;
    // mov es, ax (8E C0)
    t[0x4E] = 0x8E; t[0x4F] = 0xC0;
    // mov ss, ax (8E D0)
    t[0x50] = 0x8E; t[0x51] = 0xD0;
    // mov esp, 0x80FC (66 BC FC 80 00 00)
    t[0x52] = 0x66; t[0x53] = 0xBC; t[0x54] = 0xFC; t[0x55] = 0x80;
    t[0x56] = 0x00; t[0x57] = 0x00;
    // mov eax, cr4 (0F 20 E0)
    t[0x58] = 0x0F; t[0x59] = 0x20; t[0x5A] = 0xE0;
    // or eax, 0x20 (83 C8 20)
    t[0x5B] = 0x83; t[0x5C] = 0xC8; t[0x5D] = 0x20;
    // mov cr4, eax (0F 22 E0)
    t[0x5E] = 0x0F; t[0x5F] = 0x22; t[0x60] = 0xE0;
    // mov eax, [0x7000] (A1 00 70 00 00)
    t[0x61] = 0xA1; t[0x62] = 0x00; t[0x63] = 0x70; t[0x64] = 0x00; t[0x65] = 0x00;
    // mov cr3, eax (0F 22 D8)
    t[0x66] = 0x0F; t[0x67] = 0x22; t[0x68] = 0xD8;
    // mov ecx, 0xC0000080 (B9 80 00 00 C0)
    t[0x69] = 0xB9; t[0x6A] = 0x80; t[0x6B] = 0x00; t[0x6C] = 0x00; t[0x6D] = 0xC0;
    // rdmsr (0F 32)
    t[0x6E] = 0x0F; t[0x6F] = 0x32;
    // or eax, 0x100 (0D 00 01 00 00)
    t[0x70] = 0x0D; t[0x71] = 0x00; t[0x72] = 0x01; t[0x73] = 0x00; t[0x74] = 0x00;
    // wrmsr (0F 30)
    t[0x75] = 0x0F; t[0x76] = 0x30;
    // mov eax, cr0 (0F 20 C0)
    t[0x77] = 0x0F; t[0x78] = 0x20; t[0x79] = 0xC0;
    // or eax, 0x80000000 (0D 00 00 00 80)
    t[0x7A] = 0x0D; t[0x7B] = 0x00; t[0x7C] = 0x00; t[0x7D] = 0x00; t[0x7E] = 0x80;
    // mov cr0, eax (0F 22 C0)
    t[0x7F] = 0x0F; t[0x80] = 0x22; t[0x81] = 0xC0;
    // Far jump: EA <4B-offset> <2B-selector> — to 0x8080, selector 0x08
    t[0x82] = 0xEA;
    t[0x83] = 0x80; t[0x84] = 0x80; t[0x85] = 0x00; t[0x86] = 0x00;
    t[0x87] = 0x08; t[0x88] = 0x00;

    // ======== 64-bit long mode code (0x089+) ========
    // In 64-bit mode, lgdt [addr] encodes as: 0F 01 /2 rm.
    // For [0x803E] using mod=00, rm=101 (disp32): OF 01 15 3E 80 00 00
    t[0x89] = 0x0F; t[0x8A] = 0x01; t[0x8B] = 0x15;
    t[0x8C] = 0x3E; t[0x8D] = 0x80; t[0x8E] = 0x00; t[0x8F] = 0x00;
    // mov ax, 0x10 (66 B8 10 00)
    t[0x90] = 0x66; t[0x91] = 0xB8; t[0x92] = 0x10; t[0x93] = 0x00;
    // mov ds, ax (8E D8)
    t[0x94] = 0x8E; t[0x95] = 0xD8;
    // mov es, ax (8E C0)
    t[0x96] = 0x8E; t[0x97] = 0xC0;
    // mov ss, ax (8E D0)
    t[0x98] = 0x8E; t[0x99] = 0xD0;
    // Read APIC ID: mov rax, [0xFEE00020] (48 A1 <8B addr>)
    t[0x9A] = 0x48; t[0x9B] = 0xA1;
    t[0x9C] = 0x20; t[0x9D] = 0x00; t[0x9E] = 0xE0; t[0x9F] = 0xFE;
    t[0xA0] = 0x00; t[0xA1] = 0x00; t[0xA2] = 0x00; t[0xA3] = 0x00;
    // shr rax, 24 (48 C1 E8 18)
    t[0xA4] = 0x48; t[0xA5] = 0xC1; t[0xA6] = 0xE8; t[0xA7] = 0x18;
    // and eax, 0xFF (25 FF 00 00 00) — zeros upper 32 of RAX
    t[0xA8] = 0x25; t[0xA9] = 0xFF; t[0xAA] = 0x00;
    t[0xAB] = 0x00; t[0xAC] = 0x00;
    // mov r8d, eax (41 89 C0)
    t[0xAD] = 0x41; t[0xAE] = 0x89; t[0xAF] = 0xC0;
    // mov rax, r8 (4C 89 C0) — need rax for addressing
    t[0xB0] = 0x4C; t[0xB1] = 0x89; t[0xB2] = 0xC0;
    // shl rax, 3 (48 C1 E0 03)
    t[0xB3] = 0x48; t[0xB4] = 0xC1; t[0xB5] = 0xE0; t[0xB6] = 0x03;
    // add rax, 0x7020 (48 05 20 70 00 00)
    t[0xB7] = 0x48; t[0xB8] = 0x05; t[0xB9] = 0x20; t[0xBA] = 0x70;
    t[0xBB] = 0x00; t[0xBC] = 0x00;
    // mov r8, [rax] (4C 8B 00)
    t[0xBD] = 0x4C; t[0xBE] = 0x8B; t[0xBF] = 0x00;
    // mov rax, r8 (4C 89 C0)
    t[0xC0] = 0x4C; t[0xC1] = 0x89; t[0xC2] = 0xC0;
    // mov rdx, rax (48 89 C2)
    t[0xC3] = 0x48; t[0xC4] = 0x89; t[0xC5] = 0xC2;
    // shr rdx, 32 (48 C1 EA 20)
    t[0xC6] = 0x48; t[0xC7] = 0xC1; t[0xC8] = 0xEA; t[0xC9] = 0x20;
    // mov ecx, 0xC0000101 (B9 01 01 00 C0)
    t[0xCA] = 0xB9; t[0xCB] = 0x01; t[0xCC] = 0x01; t[0xCD] = 0x00; t[0xCE] = 0xC0;
    // wrmsr (0F 30)
    t[0xCF] = 0x0F; t[0xD0] = 0x30;
    // mov rsp, [r8] (49 8B 20) — PerCpu.kernel_rsp is field 0
    t[0xD1] = 0x49; t[0xD2] = 0x8B; t[0xD3] = 0x20;
    // mov rdi, r8 (4C 89 C7)
    t[0xD4] = 0x4C; t[0xD5] = 0x89; t[0xD6] = 0xC7;
    // mov rax, [0x7010] (48 A1 <8B addr 0x7010>)
    t[0xD7] = 0x48; t[0xD8] = 0xA1;
    t[0xD9] = 0x10; t[0xDA] = 0x70; t[0xDB] = 0x00; t[0xDC] = 0x00;
    t[0xDD] = 0x00; t[0xDE] = 0x00; t[0xDF] = 0x00; t[0xE0] = 0x00;
    // call rax (FF D0)
    t[0xE1] = 0xFF; t[0xE2] = 0xD0;
    // hlt (F4)
    t[0xE3] = 0xF4;
    // jmp -5 (EB F9) — jump back to hlt
    t[0xE4] = 0xEB; t[0xE5] = 0xF9;

    t
};

const TRAMPOLINE_SIZE: usize = 0xF0; // 240 bytes

// ---------------------------------------------------------------------------
// IPI-обработчик (вектор IPI_WAKE_VECTOR)
// ---------------------------------------------------------------------------

/// Заглушка IPI. Вызывается из IDT при получении IPI.
#[unsafe(naked)]
pub unsafe extern "C" fn ipi_stub() {
    naked_asm!(
        "push rax",
        "push rcx",
        "push rdx",
        "push rdi",
        "push rsi",
        "push r8",
        "push r9",
        "push r10",
        "push r11",
        "sub rsp, 8",
        "call {handler}",
        "add rsp, 8",
        "pop r11",
        "pop r10",
        "pop r9",
        "pop r8",
        "pop rsi",
        "pop rdi",
        "pop rdx",
        "pop rcx",
        "pop rax",
        "iretq",
        handler = sym ipi_handler,
    );
}

#[no_mangle]
unsafe extern "C" fn ipi_handler() {
    // Получить APIC ID текущего CPU — это мы (получатель).
    let apic_id = apic::lapic_id();
    let cpu_id = apic_id_to_cpu_id(apic_id);

    // Обработать очередь работы.
    process_work_queue(cpu_id);

    // EOI
    apic::eoi();
}

// ---------------------------------------------------------------------------
// Публичные функции
// ---------------------------------------------------------------------------

/// Инициализация SMP: определить число CPU, разбудить AP.
pub unsafe fn init() {
    terminal::write(b"[SMP] Detecting CPUs...\n");

    // Определить число процессоров через CPUID
    let cpu_count = crate::hw::cpu_threads() as usize;
    let cpu_count = cpu_count.clamp(1, MAX_CPUS);

    terminal::write(b"[SMP] CPU count: ");
    terminal::write_num(cpu_count as u64);
    terminal::write(b"\n");

    if cpu_count <= 1 {
        terminal::write(b"[SMP] Single-core system, SMP skipped.\n");
        CPU_COUNT = 1;
        return;
    }

    CPU_COUNT = cpu_count;

    // Инициализировать BSP PerCpu — скопировать kernel_rsp из cpu::PERCPU
    let bsp_apic_id = apic::lapic_id();
    let bsp_rsp = crate::cpu::PERCPU.kernel_rsp;
    PERCPU_ARRAY[0] = PerCpu {
        kernel_rsp: bsp_rsp,
        user_rsp_scratch: 0,
        cpu_id: 0,
        apic_id: bsp_apic_id,
        stack_base: 0,
        ap_ready: 1,
    };

    // Создать инфо-блок для трамплина на странице 0x7000
    let cr3 = cpu::read_cr3();
    let percpu_array_base = addr_of!(PERCPU_ARRAY) as u64;
    let ap_entry = ap_main as u64;

    // Записать info block
    let info = TRAMPOLINE_INFO_PHYS as *mut u64;
    write_volatile(info.add(0), cr3);            // 0x7000: cr3
    write_volatile(info.add(1), percpu_array_base); // 0x7008: percpu_array_base
    write_volatile(info.add(2), ap_entry);       // 0x7010: ap_entry

    // Записать таблицу указателей на PerCpu для каждого AP = PERCPU_ARRAY + i
    let ptr_table = (TRAMPOLINE_INFO_PHYS + 0x20) as *mut u64; // 0x7020
    for i in 1..cpu_count {
        let cpu_ptr = addr_of!(PERCPU_ARRAY[i]) as u64;
        write_volatile(ptr_table.add(i), cpu_ptr);
    }

    // Подготовить стеки для AP
    for i in 1..cpu_count {
        let stack_top = (addr_of!(AP_STACKS) as u64) + (i * AP_STACK_SIZE) as u64 + AP_STACK_SIZE as u64;
        PERCPU_ARRAY[i] = PerCpu {
            kernel_rsp: stack_top & !0xF,
            user_rsp_scratch: 0,
            cpu_id: i as u32,
            apic_id: 0,                      // заполнится при wakeup
            stack_base: stack_top,
            ap_ready: 0,
        };
    }

    // Копировать код трамплина на физическую страницу 0x8000
    let tramp_dst = TRAMPOLINE_CODE_PHYS as *mut u8;
    for i in 0..TRAMPOLINE_SIZE {
        write_volatile(tramp_dst.add(i), TRAMPOLINE[i]);
    }

    terminal::write(b"[SMP] Trampoline copied to 0x8000, waking APs...\n");

    // Разбудить AP через INIT-SIPI-SIPI протокол
    for i in 1..cpu_count {
        // В реальности APIC ID не всегда == cpu_id. Нам нужно получить реальные
        // APIC ID всех процессоров. Пока используем упрощение: перебираем все
        // возможные APIC ID (0..=MAX_CPUS) и шлём SIPI.
        // Более правильно — парсить ACPI MADT, но это сложнее.
        // Для QEMU с -smp N, APIC ID обычно = 0, 1, 2, ...
        let target_apic_id = (i as u32) & 0xFF;
        wake_ap(target_apic_id, i);
    }

    // Ждать завершения инициализации AP (простой таймаут)
    let mut waited: u64 = 0;
    while AP_READY_COUNT < cpu_count - 1 && waited < 5000 {
        // APIC tick delay
        for _ in 0..1000000 { core::hint::spin_loop(); }
        waited += 1;
    }

    terminal::write(b"[SMP] APs ready: ");
    terminal::write_num(AP_READY_COUNT as u64);
    terminal::write(b"/");
    terminal::write_num((cpu_count - 1) as u64);
    terminal::write(b"\n");

    // Установить GS_BASE для BSP на PERCPU_ARRAY[0]
    cpu::wrmsr(cpu::IA32_GS_BASE, addr_of!(PERCPU_ARRAY[0]) as u64);
    cpu::wrmsr(cpu::IA32_KERNEL_GS_BASE, 0);
}

/// Обновить вершину стека ядра текущего процесса (читается трамплином syscall).
/// Использует PERCPU_ARRAY[0] — BSP-совместимость.
pub unsafe fn set_kernel_rsp(rsp: u64) {
    PERCPU_ARRAY[0].kernel_rsp = rsp;
    // Для совместимости с cpu::PerCpu
    crate::cpu::PERCPU.kernel_rsp = rsp;
}

/// Разбудить один AP.
unsafe fn wake_ap(apic_id: u32, cpu_index: usize) {
    if apic_id == apic::lapic_id() { return; }

    terminal::write(b"[SMP] Waking AP with APIC ID ");
    terminal::write_num(apic_id as u64);
    terminal::write(b" (cpu ");
    terminal::write_num(cpu_index as u64);
    terminal::write(b")\n");

    // 1. INIT IPI
    apic::send_init_ipi(apic_id);
    // 2. Ждать ~10ms
    for _ in 0..1000000 { core::hint::spin_loop(); }
    // 3. SIPI
    apic::send_startup_ipi(apic_id, SIPI_VECTOR);
    // 4. Ждать ~200us
    for _ in 0..200000 { core::hint::spin_loop(); }
    // 5. Повторный SIPI (по спецификации)
    apic::send_startup_ipi(apic_id, SIPI_VECTOR);
}

/// Точка входа для AP (вызывается из трамплина после перехода в 64-bit режим).
#[no_mangle]
unsafe extern "C" fn ap_main(percpu: *mut PerCpu) -> ! {
    let cpu_id = (*percpu).cpu_id;
    let apic_id = apic::lapic_id();

    // Обновить APIC ID в PerCpu
    (*percpu).apic_id = apic_id;

    // Инициализировать LAPIC таймер на AP
    apic::init_ap();

    // Пометить AP готовым
    (*percpu).ap_ready = 1;
    AP_READY_COUNT += 1;

    // IPI обработчик — AP будет сидеть в цикле, обрабатывая IPI
    terminal::write(b"[SMP] AP ");
    terminal::write_num(cpu_id as u64);
    terminal::write(b" (APIC ");
    terminal::write_num(apic_id as u64);
    terminal::write(b") ready, entering idle loop.\n");

    loop {
        // Специальный хинт для гипервизора — инструкция pause
        asm!("pause", options(nomem, nostack, preserves_flags));
        // Halt until next interrupt
        asm!("hlt", options(nomem, nostack, preserves_flags));

        // После IPI обработать очередь
        process_work_queue(cpu_id as usize);
    }
}

/// Отправить IPI конкретному CPU.
pub unsafe fn send_ipi_to_cpu(cpu_id: usize, vector: u8) {
    if cpu_id >= MAX_CPUS { return; }
    let apic_id = PERCPU_ARRAY[cpu_id].apic_id;
    if apic_id == 0 && cpu_id != 0 { return; } // AP не готов
    apic::send_ipi(apic_id, vector);
}

/// Отправить IPI всем остальным CPU.
pub unsafe fn send_ipi_broadcast(vector: u8) {
    apic::send_ipi_all_others(vector);
}

// ---------------------------------------------------------------------------
// Work queue
// ---------------------------------------------------------------------------

/// Добавить работу в очередь указанного CPU.
pub unsafe fn push_work(cpu_id: usize, handler: WorkFn, arg: u64) -> bool {
    let tail = WORK_QUEUE_TAILS[cpu_id];
    let head = WORK_QUEUE_HEADS[cpu_id];
    let next = (tail + 1) % WORK_QUEUE_SIZE;
    if next == head { return false; } // full

    WORK_QUEUES[cpu_id][tail] = WorkItem { handler: Some(handler), arg };
    WORK_QUEUE_TAILS[cpu_id] = next;
    true
}

/// Обработать очередь работы (вызывается по IPI).
unsafe fn process_work_queue(cpu_id: usize) {
    while WORK_QUEUE_HEADS[cpu_id] != WORK_QUEUE_TAILS[cpu_id] {
        let head = WORK_QUEUE_HEADS[cpu_id];
        let item = &WORK_QUEUES[cpu_id][head];
        if let Some(f) = item.handler {
            f(item.arg);
        }
        WORK_QUEUE_HEADS[cpu_id] = (head + 1) % WORK_QUEUE_SIZE;
    }
}

// ---------------------------------------------------------------------------
// Вспомогательные функции
// ---------------------------------------------------------------------------

/// Найти cpu_id по APIC ID (линейный поиск).
unsafe fn apic_id_to_cpu_id(apic_id: u32) -> usize {
    for i in 0..MAX_CPUS {
        if PERCPU_ARRAY[i].apic_id == apic_id {
            return i;
        }
    }
    0 // default to BSP
}

/// Получить количество процессоров.
pub fn cpu_count() -> usize {
    unsafe { CPU_COUNT }
}

/// Получить cpu_id текущего CPU (по APIC ID).
pub unsafe fn current_cpu_id() -> usize {
    let id = apic::lapic_id();
    apic_id_to_cpu_id(id)
}

/// Получить PerCpu текущего CPU.
pub unsafe fn current_percpu() -> *mut PerCpu {
    let cpu_id = current_cpu_id();
    addr_of_mut!(PERCPU_ARRAY[cpu_id])
}
