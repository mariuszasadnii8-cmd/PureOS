//! SMP — symmetric multiprocessing.
//!
//! AP wakeup via INIT-SIPI-SIPI protocol with **broadcast** (ICR_DEST_ALL_EXCL_SELF),
//! avoiding assumption that APIC IDs are sequential. Each AP atomically claims
//! a free slot from a counter at 0x7018 using `lock xadd`.
//!
//! Trampoline at physical 0x8000 transitions APs from real mode → long mode.
//! Per-CPU data, work queue, and IPI dispatch.
//!
//! Zero-Alloc: all structures are static arrays of fixed size.

use core::arch::{asm, naked_asm};
use core::ptr::{addr_of, write_bytes, write_volatile, addr_of_mut};

use crate::apic;
use crate::cpu;
use crate::frame;
use crate::terminal;

// ---------------------------------------------------------------------------
// Конфигурация SMP
// ---------------------------------------------------------------------------

/// Максимальное число логических процессоров (поддержка до 16 ядер/потоков).
pub const MAX_CPUS: usize = 16;

/// Физический адрес страницы с информацией для AP (cr3, slot_counter, ap_entry).
/// Используем 0x1000 (первая страница после IVT/BDA — обычно свободна).
const TRAMPOLINE_INFO_PHYS: u64 = 0x1000;

/// Физический адрес страницы с кодом трамплина (SIPI vector 8 = 0x8000).
const TRAMPOLINE_CODE_PHYS: u64 = 0x8000;

/// SIPI vector для wakeup AP (адрес = vector * 0x1000 = 0x8000).
const SIPI_VECTOR: u8 = 0x08;

/// Размер стека ядра на каждый AP.
const AP_STACK_SIZE: usize = 16384;

/// Максимум элементов в очереди работы (IPI).
const WORK_QUEUE_SIZE: usize = 32;

/// Количество попыток ожидания AP (каждая ~10ms спинов).
const AP_WAIT_ATTEMPTS: u64 = 500;

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
///
/// В 64-битном режиме каждый AP атомарно забирает слот из счётчика на 0x1018
/// (lock xadd) и получает свой PerCpu из таблицы по индексу слота. Это
/// избавляет от предположения, что APIC ID == cpu index.
///
/// Разметка на странице 0x8000:
///   0x000: 16-bit code (≈31 байт)
///   0x020: GDT: null | kcode32 | kdata32 | kcode64 (32 байта, 4 дескриптора)
///   0x040: GDTP 16 (6 байт: limit=31, base=0x8020)
///   0x048: GDTP 64 (10 байт: limit=31, base=0x8020)
///   0x060: 32-bit protected mode code (≈65 байт)
///   0x0A1: 64-bit long mode code
///
/// ИСПРАВЛЕНИЯ для реального железа (QEMU эти баги не ловит):
///   1-5: см. комментарии ниже в коде.
///
/// Инфоблок на 0x1000:
///   0x1000: u64 cr3_pae (32-bit PAE PDPT address, должен быть <4GB)
///   0x1008: u64 percpu_array_base (unused by trampoline)
///   0x1010: u64 ap_main entry
///   0x1018: u64 slot_counter (atomic, init=1)
///   0x1020: u64[MAX_CPUS] percpu_ptr_table
///   0x10A0: u64 cr3_64 (полный 64-битный CR3 для перезагрузки после входа в LM)
const TRAMPOLINE: [u8; 512] = {
    let mut t = [0u8; 512];

    // ======== 16-bit real mode code (0x00–0x1E) ========
    // cli
    t[0] = 0xFA;
    // cld
    t[1] = 0xFC;
    // xor ax, ax
    t[2] = 0x31; t[3] = 0xC0;
    // mov ds, ax
    t[4] = 0x8E; t[5] = 0xD8;
    // mov es, ax
    t[6] = 0x8E; t[7] = 0xC0;
    // mov ss, ax
    t[8] = 0x8E; t[9] = 0xD0;
    // lgdt [0x8040] — ModRM 0x16 (16-bit disp16), not 0x15
    t[0x0A] = 0x0F; t[0x0B] = 0x01; t[0x0C] = 0x16;
    t[0x0D] = 0x40; t[0x0E] = 0x80;
    // mov eax, cr0
    t[0x0F] = 0x0F; t[0x10] = 0x20; t[0x11] = 0xC0;
    // or al, 1
    t[0x12] = 0x0C; t[0x13] = 0x01;
    // mov cr0, eax
    t[0x14] = 0x0F; t[0x15] = 0x22; t[0x16] = 0xC0;
    // jmp far ptr16:32 0x08:0x8060 (to 32-bit code segment)
    t[0x17] = 0x66; t[0x18] = 0xEA;
    t[0x19] = 0x60; t[0x1A] = 0x80; t[0x1B] = 0x00; t[0x1C] = 0x00;
    t[0x1D] = 0x08; t[0x1E] = 0x00;

    // ======== GDT at 0x20 (32 bytes, 4 entries) ========
    // null descriptor
    t[0x20] = 0x00; t[0x21] = 0x00; t[0x22] = 0x00; t[0x23] = 0x00;
    t[0x24] = 0x00; t[0x25] = 0x00; t[0x26] = 0x00; t[0x27] = 0x00;
    // 32-bit code (index 1, selector 0x08): base=0, limit=4G, G=1, D=1, L=0
    t[0x28] = 0xFF; t[0x29] = 0xFF; t[0x2A] = 0x00; t[0x2B] = 0x00;
    t[0x2C] = 0x00; t[0x2D] = 0x9A; t[0x2E] = 0xCF; t[0x2F] = 0x00;
    // data (index 2, selector 0x10): base=0, limit=4G, writable
    t[0x30] = 0xFF; t[0x31] = 0xFF; t[0x32] = 0x00; t[0x33] = 0x00;
    t[0x34] = 0x00; t[0x35] = 0x92; t[0x36] = 0xCF; t[0x37] = 0x00;
    // 64-bit code (index 3, selector 0x18): base=0, limit=4G, L=1, D=0
    t[0x38] = 0xFF; t[0x39] = 0xFF; t[0x3A] = 0x00; t[0x3B] = 0x00;
    t[0x3C] = 0x00; t[0x3D] = 0x9A; t[0x3E] = 0xAF; t[0x3F] = 0x00;

    // ======== GDTP 16 at 0x40 (6 bytes) ========
    t[0x40] = 0x1F; t[0x41] = 0x00;                 // limit = 31
    t[0x42] = 0x20; t[0x43] = 0x80; t[0x44] = 0x00; t[0x45] = 0x00;

    // ======== GDTP 64 at 0x48 (10 bytes) ========
    t[0x48] = 0x1F; t[0x49] = 0x00;
    t[0x4A] = 0x20; t[0x4B] = 0x80; t[0x4C] = 0x00; t[0x4D] = 0x00;
    t[0x4E] = 0x00; t[0x4F] = 0x00; t[0x50] = 0x00; t[0x51] = 0x00;

    // ======== 32-bit protected mode code at 0x60 ========
    // mov ax, 0x10 (data selector)
    t[0x60] = 0x66; t[0x61] = 0xB8; t[0x62] = 0x10; t[0x63] = 0x00;
    // mov ds, ax
    t[0x64] = 0x8E; t[0x65] = 0xD8;
    // mov es, ax
    t[0x66] = 0x8E; t[0x67] = 0xC0;
    // mov ss, ax
    t[0x68] = 0x8E; t[0x69] = 0xD0;
    // mov esp, 0x80FC (temp stack)
    t[0x6A] = 0x66; t[0x6B] = 0xBC; t[0x6C] = 0xFC; t[0x6D] = 0x80;
    t[0x6E] = 0x00; t[0x6F] = 0x00;
    // mov eax, cr4
    t[0x70] = 0x0F; t[0x71] = 0x20; t[0x72] = 0xE0;
    // or eax, 0x20 (PAE)
    t[0x73] = 0x83; t[0x74] = 0xC8; t[0x75] = 0x20;
    // mov cr4, eax
    t[0x76] = 0x0F; t[0x77] = 0x22; t[0x78] = 0xE0;
    // mov eax, [0x1000] (CR3_PAE from info block — гарантированно <4GB)
    t[0x79] = 0xA1; t[0x7A] = 0x00; t[0x7B] = 0x10; t[0x7C] = 0x00; t[0x7D] = 0x00;
    // mov cr3, eax
    t[0x7E] = 0x0F; t[0x7F] = 0x22; t[0x80] = 0xD8;
    // mov ecx, IA32_EFER
    t[0x81] = 0xB9; t[0x82] = 0x80; t[0x83] = 0x00; t[0x84] = 0x00; t[0x85] = 0xC0;
    // rdmsr
    t[0x86] = 0x0F; t[0x87] = 0x32;
    // or eax, 0x100 (LME)
    t[0x88] = 0x0D; t[0x89] = 0x00; t[0x8A] = 0x01; t[0x8B] = 0x00; t[0x8C] = 0x00;
    // wrmsr
    t[0x8D] = 0x0F; t[0x8E] = 0x30;
    // mov eax, cr0
    t[0x8F] = 0x0F; t[0x90] = 0x20; t[0x91] = 0xC0;
    // or eax, 0x80000000 (PG)
    t[0x92] = 0x0D; t[0x93] = 0x00; t[0x94] = 0x00; t[0x95] = 0x00; t[0x96] = 0x80;
    // mov cr0, eax
    t[0x97] = 0x0F; t[0x98] = 0x22; t[0x99] = 0xC0;
    // jmp far 0x18:0x80A1 (enter 64-bit, selector 0x18 = kcode64)
    t[0x9A] = 0xEA;
    t[0x9B] = 0xA1; t[0x9C] = 0x80; t[0x9D] = 0x00; t[0x9E] = 0x00;
    t[0x9F] = 0x18; t[0xA0] = 0x00;

    // ======== 64-bit long mode code at 0xA1 ========
    // Patch GDT entry 1 → 64-bit CS so KERNEL_CS=0x08 works on AP
    t[0xA1] = 0x48; t[0xA2] = 0xB8;
    t[0xA3] = 0xFF; t[0xA4] = 0xFF; t[0xA5] = 0x00; t[0xA6] = 0x00;
    t[0xA7] = 0x00; t[0xA8] = 0x9A; t[0xA9] = 0xAF; t[0xAA] = 0x00;
    // mov [0x8028], rax
    t[0xAB] = 0x48; t[0xAC] = 0xA3;
    t[0xAD] = 0x28; t[0xAE] = 0x80; t[0xAF] = 0x00; t[0xB0] = 0x00;
    t[0xB1] = 0x00; t[0xB2] = 0x00; t[0xB3] = 0x00; t[0xB4] = 0x00;
    // lgdt [0x8048] — ModRM 0x14+SIB (absolute [disp32]), not [rip+disp32]
    t[0xB5] = 0x0F; t[0xB6] = 0x01; t[0xB7] = 0x14; t[0xB8] = 0x25;
    t[0xB9] = 0x48; t[0xBA] = 0x80; t[0xBB] = 0x00; t[0xBC] = 0x00;
    // mov ax, 0x10; reload data segments
    t[0xBD] = 0x66; t[0xBE] = 0xB8; t[0xBF] = 0x10; t[0xC0] = 0x00;
    t[0xC1] = 0x8E; t[0xC2] = 0xD8;
    t[0xC3] = 0x8E; t[0xC4] = 0xC0;
    t[0xC5] = 0x8E; t[0xC6] = 0xD0;
    // mov eax, 1
    t[0xC7] = 0xB8; t[0xC8] = 0x01; t[0xC9] = 0x00; t[0xCA] = 0x00; t[0xCB] = 0x00;
    // lock xadd qword [0x1018], rax (claim slot)
    t[0xCC] = 0xF0; t[0xCD] = 0x48; t[0xCE] = 0x0F; t[0xCF] = 0xC1;
    t[0xD0] = 0x04; t[0xD1] = 0x25; t[0xD2] = 0x18; t[0xD3] = 0x10;
    t[0xD4] = 0x00; t[0xD5] = 0x00;
    // shl rax, 3
    t[0xD6] = 0x48; t[0xD7] = 0xC1; t[0xD8] = 0xE0; t[0xD9] = 0x03;
    // add rax, 0x1020 (offset of percpu_ptr_table)
    t[0xDA] = 0x48; t[0xDB] = 0x05; t[0xDC] = 0x20; t[0xDD] = 0x10;
    t[0xDE] = 0x00; t[0xDF] = 0x00;
    // mov r8, [rax] (r8 = PerCpu*)
    t[0xE0] = 0x4C; t[0xE1] = 0x8B; t[0xE2] = 0x00;
    // mov rdx, r8 (compressed from mov rax,r8 + mov rdx,rax)
    t[0xE3] = 0x4C; t[0xE4] = 0x89; t[0xE5] = 0xC2;
    // shr rdx, 32
    t[0xE6] = 0x48; t[0xE7] = 0xC1; t[0xE8] = 0xEA; t[0xE9] = 0x20;
    // mov ecx, IA32_GS_BASE
    t[0xEA] = 0xB9; t[0xEB] = 0x01; t[0xEC] = 0x01; t[0xED] = 0x00; t[0xEE] = 0xC0;
    // wrmsr (set GS_BASE = PerCpu*)
    t[0xEF] = 0x0F; t[0xF0] = 0x30;
    // mov rsp, [r8] (PerCpu.kernel_rsp)
    t[0xF1] = 0x49; t[0xF2] = 0x8B; t[0xF3] = 0x20;
    // --- Reload CR3 with full 64-bit value from info block ---
    // mov rax, [0x10A0] (cr3_64 — BSP's full CR3)
    t[0xF4] = 0x48; t[0xF5] = 0xA1;
    t[0xF6] = 0xA0; t[0xF7] = 0x10; t[0xF8] = 0x00; t[0xF9] = 0x00;
    t[0xFA] = 0x00; t[0xFB] = 0x00; t[0xFC] = 0x00; t[0xFD] = 0x00;
    // mov cr3, rax
    t[0xFE] = 0x0F; t[0xFF] = 0x22; t[0x100] = 0xD8;
    // mov rdi, r8 (first arg = PerCpu*)
    t[0x101] = 0x4C; t[0x102] = 0x89; t[0x103] = 0xC7;
    // mov rax, [0x1010] (ap_main entry)
    t[0x104] = 0x48; t[0x105] = 0xA1;
    t[0x106] = 0x10; t[0x107] = 0x10; t[0x108] = 0x00; t[0x109] = 0x00;
    t[0x10A] = 0x00; t[0x10B] = 0x00; t[0x10C] = 0x00; t[0x10D] = 0x00;
    // call rax
    t[0x10E] = 0xFF; t[0x10F] = 0xD0;
    // hlt
    t[0x110] = 0xF4;
    // jmp -3 (back to hlt)
    t[0x111] = 0xEB; t[0x112] = 0xFD;

    t
};

const TRAMPOLINE_SIZE: usize = 0x120;

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
    let apic_id = apic::lapic_id();
    let cpu_id = apic_id_to_cpu_id(apic_id);
    process_work_queue(cpu_id);
    apic::eoi();
}

// ---------------------------------------------------------------------------
// Публичные функции
// ---------------------------------------------------------------------------

/// Инициализация SMP: определить число CPU, разбудить AP (broadcast).
pub unsafe fn init() {
    terminal::write(b"[SMP] Detecting CPUs...\n");

    let raw_count = crate::hw::cpu_threads() as usize;
    let cpu_count = raw_count.clamp(1, MAX_CPUS);

    terminal::write(b"[SMP] CPU count: ");
    terminal::write_num(raw_count as u64);
    terminal::write(b" detected, using ");
    terminal::write_num(cpu_count as u64);
    terminal::write(b"\n");

    if cpu_count <= 1 {
        terminal::write(b"[SMP] Single-core system, SMP skipped.\n");
        CPU_COUNT = 1;
        return;
    }

    // ═══════════════════════════════════════════════════════════════════
    // 1. Инициализировать BSP PerCpu (слот 0)
    // ═══════════════════════════════════════════════════════════════════
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

    // ═══════════════════════════════════════════════════════════════════
    // 2. Подготовить стеки и PerCpu для всех AP-слотов
    // ═══════════════════════════════════════════════════════════════════
    for i in 1..MAX_CPUS {
        let stack_top = (addr_of!(AP_STACKS) as u64)
            + (i * AP_STACK_SIZE) as u64
            + AP_STACK_SIZE as u64;
        PERCPU_ARRAY[i] = PerCpu {
            kernel_rsp: stack_top & !0xF,
            user_rsp_scratch: 0,
            cpu_id: i as u32,
            apic_id: 0,
            stack_base: stack_top,
            ap_ready: 0,
        };
    }

    // ═══════════════════════════════════════════════════════════════════
    // 5. Создать PAE-совместимую PDPT + PD в low memory (frame allocator
    //    гарантирует <4GB → 32-bit CR3 read работает).
    //    Identity-map 0-128MB (2MB pages) — покрывает трамплин (0x8000),
    //    ядро (0x2000000), AP_STACKS, PERCPU_ARRAY.
    // ═══════════════════════════════════════════════════════════════════
    let pdpt_frame = frame::alloc_frame()
        .expect("[SMP] FATAL: cannot allocate PDPT frame for AP");
    let pd_frame = frame::alloc_frame()
        .expect("[SMP] FATAL: cannot allocate PD frame for AP");

    // PDPT: 4 entries at pdpt_frame (entry 0 = valid, rest = 0 = not present)
    // In PAE mode, PDPT[0] covers virtual 0-1GB.
    // PDPTE format: same as PML4E; bits 51:12 = PD base addr.
    let pdpt = pdpt_frame as *mut u64;
    write_bytes(pdpt_frame as *mut u8, 0, 4096);
    let pd_base = pd_frame & 0x000F_FFFF_FFFF_F000;
    *pdpt.add(0) = pd_base | 0x03; // Present | Writable

    // PD: 512 entries, each 2MB page identity-mapping
    let pd = pd_frame as *mut u64;
    write_bytes(pd_frame as *mut u8, 0, 4096); // clear all entries first
    for i in 0..64 {
        // Map 64 * 2MB = 128MB identity (covers 0-128MB)
        let phys_2mb = (i as u64) * 0x200000;
        *pd.add(i) = phys_2mb | 0x83; // Present | Writable | PS (2MB page)
    }

    terminal::write(b"[SMP] AP PDPT at ");
    terminal::write_num(pdpt_frame);
    terminal::write(b", PD at ");
    terminal::write_num(pd_frame);
    terminal::write(b"\n");

    // ═══════════════════════════════════════════════════════════════════
    // 6. Записать инфо-блок для трамплина на странице 0x1000
    // ═══════════════════════════════════════════════════════════════════
    let bsp_cr3 = cpu::read_cr3();
    terminal::write(b"[SMP] BSP CR3: 0x");
    terminal::write_hex(bsp_cr3);
    terminal::write(b"\n");
    terminal::write(b"[SMP] x2APIC: ");
    if apic::x2apic_enabled() {
        terminal::write(b"ENABLED (Kaby Lake may need per-APIC-ID wakeup)\n");
    } else {
        terminal::write(b"disabled (xAPIC mode)\n");
    }
    let ap_entry = ap_main as u64;
    let percpu_array_base = addr_of!(PERCPU_ARRAY) as u64;

    let info = TRAMPOLINE_INFO_PHYS as *mut u64;
    // Offset 0: CR3_PAE (адрес PDPT, <4GB, читается 32-bit mov eax, [0x1000])
    write_volatile(info.add(0), pdpt_frame);
    // Offset 1: percpu_array_base (unused by trampoline)
    write_volatile(info.add(1), percpu_array_base);
    // Offset 2: ap_main entry
    write_volatile(info.add(2), ap_entry);
    // Offset 3: slot_counter (init=1)
    write_volatile(info.add(3), 1);

    // Offset 0x20-0x9F: ptr_table для всех слотов
    let ptr_table = (TRAMPOLINE_INFO_PHYS + 0x20) as *mut u64;
    for slot in 0..MAX_CPUS {
        let cpu_ptr = addr_of!(PERCPU_ARRAY[slot]) as u64;
        write_volatile(ptr_table.add(slot), cpu_ptr);
    }

    // Offset 0xA0: CR3_64 (полный 64-битный CR3 BSP для перезагрузки
    // после входа в Long Mode — решает проблему PML4 выше 4GB)
    let cr3_64 = (TRAMPOLINE_INFO_PHYS + 0xA0) as *mut u64;
    write_volatile(cr3_64, bsp_cr3);

    // ═══════════════════════════════════════════════════════════════════
    // 7. Кэш: WBINVD + копирование трамплина + CLFLUSH
    // ═══════════════════════════════════════════════════════════════════
    // WBINVD сначала, чтобы сбросить возможные stale-линии,
    // затем пишем трамплин, затем CLFLUSH для гарантии видимости.
    asm!("wbinvd", options(nomem, nostack, preserves_flags));

    let tramp_dst = TRAMPOLINE_CODE_PHYS as *mut u8;
    for i in 0..TRAMPOLINE_SIZE {
        write_volatile(tramp_dst.add(i), TRAMPOLINE[i]);
    }

    // CLFLUSH для страницы трамплина
    for i in (0..TRAMPOLINE_SIZE).step_by(64) {
        let addr = tramp_dst.add(i) as u64;
        asm!("clflush [{addr}]", addr = in(reg) addr, options(nomem, nostack, preserves_flags));
    }
    // CLFLUSH для инфо-страницы (0x1000)
    for i in (0..512).step_by(64) {
        let addr = (TRAMPOLINE_INFO_PHYS + i as u64) as u64;
        asm!("clflush [{addr}]", addr = in(reg) addr, options(nomem, nostack, preserves_flags));
    }
    // CLFLUSH для PDPT (pdpt_frame) и PD (pd_frame) — другая страница
    for i in (0..4096).step_by(64) {
        let addr = pdpt_frame + i;
        asm!("clflush [{addr}]", addr = in(reg) addr, options(nomem, nostack, preserves_flags));
        if (pdpt_frame..pdpt_frame + 4096).contains(&(pd_frame + i)) {
            continue; // если PD в той же странице — не дублируем
        }
        let addr2 = pd_frame + i;
        asm!("clflush [{addr2}]", addr2 = in(reg) addr2, options(nomem, nostack, preserves_flags));
    }
    asm!("sfence", options(nomem, nostack, preserves_flags));

    terminal::write(b"[SMP] Trampoline at 0x8000, waking APs...\n");

    // ═══════════════════════════════════════════════════════════════════
    // 8. Попытка 1: per-APIC-ID INIT/SIPI через ACPI MADT
    //    Это обходит x2APIC destination-shorthand errata (KBL091 и др.)
    // ═══════════════════════════════════════════════════════════════════
    let mut apic_id_buf = [0u32; MAX_CPUS];
    let mut found_aps = 0usize;
    let st_addr = crate::uefi::system_table_addr();
    if st_addr != 0 {
        let rsdp = crate::acpi::find_rsdp(st_addr);
        if crate::acpi::validate_rsdp(rsdp) {
            let madt = crate::acpi::find_table(rsdp, b"APIC");
            found_aps = crate::acpi::parse_madt(madt, &mut apic_id_buf);
            terminal::write(b"[SMP] ACPI MADT found ");
            terminal::write_num(found_aps as u64);
            terminal::write(b" AP entries\n");
        } else {
            terminal::write(b"[SMP] ACPI RSDP not found\n");
        }
    }

    if found_aps > 0 {
        // Per-APIC-ID wakeup
        terminal::write(b"[SMP] Waking APs individually via ACPI APIC IDs\n");

        // INIT DE-ASSERT на каждый AP (кроме BSP) — очистка залипшего состояния
        for i in 0..found_aps {
            if apic_id_buf[i] != bsp_apic_id {
                apic::send_init_deassert(apic_id_buf[i]);
            }
        }
        delay_apic(200);

        // INIT ASSERT на каждый AP (кроме BSP) — сбрасывает AP в Wait-for-SIPI
        for i in 0..found_aps {
            if apic_id_buf[i] != bsp_apic_id {
                apic::send_init_ipi(apic_id_buf[i]);
            }
        }
        delay_apic(50_000); // min 10ms; 50ms для реального железа

        // SIPI 1
        for i in 0..found_aps {
            if apic_id_buf[i] != bsp_apic_id {
                apic::send_startup_ipi(apic_id_buf[i], SIPI_VECTOR);
            }
        }
        delay_apic(500);

        // SIPI 2
        for i in 0..found_aps {
            if apic_id_buf[i] != bsp_apic_id {
                apic::send_startup_ipi(apic_id_buf[i], SIPI_VECTOR);
            }
        }
        delay_apic(500);
    } else {
        // ═══════════════════════════════════════════════════════════════════
        // Fallback: broadcast INIT-SIPI-SIPI
        // ═══════════════════════════════════════════════════════════════════
        terminal::write(b"[SMP] No ACPI, falling back to broadcast wakeup\n");

        apic::send_init_deassert_all_others();
        delay_apic(200);
        apic::send_init_ipi_all_others();
        delay_apic(10_000);
        apic::send_startup_ipi_all_others(SIPI_VECTOR);
        delay_apic(300);
        apic::send_startup_ipi_all_others(SIPI_VECTOR);
        delay_apic(300);
    }

    // ═══════════════════════════════════════════════════════════════════
    // 9. Ждать AP с таймаутом
    // ═══════════════════════════════════════════════════════════════════
    let mut waited: u64 = 0;
    while AP_READY_COUNT < cpu_count - 1 && waited < AP_WAIT_ATTEMPTS {
        delay_apic(10_000);
        waited += 1;
        if waited % 50 == 0 {
            terminal::write(b"[SMP] Waiting for APs... (");
            terminal::write_num(AP_READY_COUNT as u64);
            terminal::write(b"/");
            terminal::write_num((cpu_count - 1) as u64);
            terminal::write(b" ready, attempt ");
            terminal::write_num(waited);
            terminal::write(b")\n");
        }
    }

    if AP_READY_COUNT >= cpu_count - 1 {
        terminal::write(b"[SMP] All APs ready: ");
        terminal::write_num(AP_READY_COUNT as u64);
        terminal::write(b"/");
        terminal::write_num((cpu_count - 1) as u64);
        terminal::write(b"\n");
    } else {
        terminal::write(b"[SMP] WARNING: Only ");
        terminal::write_num(AP_READY_COUNT as u64);
        terminal::write(b"/");
        terminal::write_num((cpu_count - 1) as u64);
        terminal::write(b" APs woke up. Continuing with available CPUs.\n");
        CPU_COUNT = 1 + AP_READY_COUNT;
    }

    // Установить GS_BASE для BSP на PERCPU_ARRAY[0]
    cpu::wrmsr(cpu::IA32_GS_BASE, addr_of!(PERCPU_ARRAY[0]) as u64);
    cpu::wrmsr(cpu::IA32_KERNEL_GS_BASE, 0);
}

/// Задержка через APIC timer (точнее, чем raw spin-loop).
/// Использует APIC CURRENT COUNT для измерения ~микросекунд.
unsafe fn delay_apic(us: u64) {
    // Если APIC калиброван, используем его для точной задержки.
    let calibrated = apic::calibrated_count() as u64;
    if calibrated > 0 && calibrated < 1_000_000_000 {
        // calibrated_count — количество APIC тиков за ~10ms.
        // Тиков за микросекунду ≈ calibrated / 10_000
        let ticks_per_us = calibrated / 10_000;
        if ticks_per_us > 0 {
            let target_us = us.min(100_000); // max 100ms за раз
            let target_ticks = target_us * ticks_per_us;
            // Однократный режим
            apic::lapic_write(apic::LAPIC_LVT_TIMER, apic::TIMER_VECTOR as u32);
            apic::lapic_write(apic::LAPIC_DIVIDE_CONFIG, 0x0B);
            apic::lapic_write(apic::LAPIC_INIT_COUNT, target_ticks as u32);
            // Ждать, пока счётчик не дойдёт до 0
            loop {
                let current = apic::lapic_read(apic::LAPIC_CURRENT_COUNT);
                if current == 0 { break; }
                core::hint::spin_loop();
            }
            // Выключить таймер
            apic::lapic_write(apic::LAPIC_LVT_TIMER, 0x10000); // masked
            return;
        }
    }
    // Fallback: spin-loop (примерно 1 итерация ≈ 1ns на 2GHz)
    // us * 2000 = количество итераций для ~us микросекунд на 2GHz
    let iterations = us * 2000;
    for _ in 0..iterations {
        core::hint::spin_loop();
    }
}

/// Обновить вершину стека ядра текущего процесса (читается трамплином syscall).
pub unsafe fn set_kernel_rsp(rsp: u64) {
    PERCPU_ARRAY[0].kernel_rsp = rsp;
    crate::cpu::PERCPU.kernel_rsp = rsp;
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

    terminal::write(b"[SMP] AP ");
    terminal::write_num(cpu_id as u64);
    terminal::write(b" (APIC ");
    terminal::write_num(apic_id as u64);
    terminal::write(b") ready, entering idle loop.\n");

    loop {
        asm!("pause", options(nomem, nostack, preserves_flags));
        asm!("hlt", options(nomem, nostack, preserves_flags));
        process_work_queue(cpu_id as usize);
    }
}

/// Отправить IPI конкретному CPU.
pub unsafe fn send_ipi_to_cpu(cpu_id: usize, vector: u8) {
    if cpu_id >= MAX_CPUS { return; }
    let apic_id = PERCPU_ARRAY[cpu_id].apic_id;
    if apic_id == 0 && cpu_id != 0 { return; }
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
    if next == head { return false; }
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
    0
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
