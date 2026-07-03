//! APIC-таймер — основа вытесняющего планировщика.
//!
//! Настройка локального APIC (LAPIC) через memory-mapped регистры.
//! LAPIC-таймер генерирует прерывание по вектору TIMER_VECTOR,
//! которое вызывает `timer_tick()` для вытеснения текущего процесса.
//! Также предоставляет IPI-функции для SMP (INIT, SIPI, фиксированные IPI).
//!
//! ⚠ Включение прерываний (sti) разрешено ТОЛЬКО после настройки APIC
//! и установки обработчика в IDT. До этого — кооперативный режим (IF=0).

use core::ptr::{read_volatile, write_volatile};

// LAPIC базовый адрес (обычно 0xFEE00000 для x86)
const LAPIC_BASE: u64 = 0xFEE0_0000;
const LAPIC_SIZE: u64 = 0x1000; // 4KB

// Смещения регистров LAPIC (в байтах)
const LAPIC_ID: u64 = 0x020;
const LAPIC_VERSION: u64 = 0x030;
const LAPIC_EOI: u64 = 0x0B0;
const LAPIC_SPURIOUS: u64 = 0x0F0;
const LAPIC_ICR_LOW: u64 = 0x300;
const LAPIC_ICR_HIGH: u64 = 0x310;
const LAPIC_LVT_TIMER: u64 = 0x320;
const LAPIC_INIT_COUNT: u64 = 0x380;
const LAPIC_CURRENT_COUNT: u64 = 0x390;
const LAPIC_DIVIDE_CONFIG: u64 = 0x3E0;

const LAPIC_ENABLE_BIT: u32 = 0x100;
const TIMER_PERIODIC: u32 = 0x20000;

// ICR delivery mode (биты 8-10 в ICR_LOW)
const ICR_FIXED: u32 = 0 << 8;
const ICR_INIT: u32 = 5 << 8;
const ICR_SIPI: u32 = 6 << 8;

// ICR destination shorthand (биты 18-19)
const ICR_DEST_FIELD: u32 = 0 << 18;
const ICR_DEST_ALL: u32 = 1 << 18;
const ICR_DEST_ALL_OTHER: u32 = 2 << 18;

const ICR_ASSERT: u32 = 1 << 14;
const ICR_LEVEL: u32 = 1 << 15;
const ICR_TRIGGER_LEVEL: u32 = 1 << 15;

/// Вектор прерывания таймера.
pub const TIMER_VECTOR: u8 = 0x20;

/// Вектор IPI (SMP wakeup / work dispatch).
pub const IPI_WAKE_VECTOR: u8 = 0x21;

static mut APIC_READY: bool = false;

#[inline(always)]
unsafe fn lapic_read(reg: u64) -> u32 {
    read_volatile((LAPIC_BASE + reg) as *const u32)
}

#[inline(always)]
unsafe fn lapic_write(reg: u64, val: u32) {
    write_volatile((LAPIC_BASE + reg) as *mut u32, val);
}

/// Прочитать APIC ID текущего процессора (BSP или AP).
pub unsafe fn lapic_id() -> u32 {
    (lapic_read(LAPIC_ID) >> 24) & 0xFF
}

/// Инициализировать LAPIC: включить, настроить таймер.
/// НЕ ИСПОЛЬЗУЕТСЯ: ядро использует кооперативный планировщик.
pub unsafe fn init() {
    // APIC timer отключен для избежания конфликтов с UEFI
    APIC_READY = false;
}

/// Инициализировать LAPIC на AP (только таймер, spurious уже включён ядром).
pub unsafe fn init_ap() {
    lapic_write(LAPIC_DIVIDE_CONFIG, 0x0B);
    lapic_write(LAPIC_LVT_TIMER, TIMER_PERIODIC | TIMER_VECTOR as u32);
    lapic_write(LAPIC_INIT_COUNT, 10_000_000);
}

/// Сигнал конца прерывания (EOI).
pub unsafe fn eoi() {
    lapic_write(LAPIC_EOI, 0);
}

/// Готова ли APIC.
pub fn is_ready() -> bool {
    unsafe { APIC_READY }
}

/// Обработчик тика таймера. Вызывается из IDT.
pub unsafe fn timer_tick() {
    if !APIC_READY { return; }
    crate::syscall::timer_tick_handler();
    eoi();
}

/// Ожидать готовность ICR (предыдущий IPI завершён).
unsafe fn icr_wait() {
    loop {
        if lapic_read(LAPIC_ICR_LOW) & (1 << 12) == 0 {
            break;
        }
        core::hint::spin_loop();
    }
}

/// Отправить IPI конкретному APIC ID (фиксированный вектор).
pub unsafe fn send_ipi(apic_id: u32, vector: u8) {
    icr_wait();
    lapic_write(LAPIC_ICR_HIGH, (apic_id as u32) << 24);
    lapic_write(LAPIC_ICR_LOW, ICR_FIXED | ICR_ASSERT | vector as u32);
    icr_wait();
}

/// Отправить INIT IPI конкретному APIC ID (для AP wakeup).
pub unsafe fn send_init_ipi(apic_id: u32) {
    icr_wait();
    lapic_write(LAPIC_ICR_HIGH, (apic_id as u32) << 24);
    lapic_write(LAPIC_ICR_LOW, ICR_INIT | ICR_LEVEL | ICR_TRIGGER_LEVEL);
    icr_wait();
}

/// Отправить SIPI конкретному APIC ID с вектором (адрес = vector * 0x1000).
pub unsafe fn send_startup_ipi(apic_id: u32, vector: u8) {
    icr_wait();
    lapic_write(LAPIC_ICR_HIGH, (apic_id as u32) << 24);
    lapic_write(LAPIC_ICR_LOW, ICR_SIPI | vector as u32);
    icr_wait();
}

/// Отправить IPI всем остальным процессорам (broadcast, кроме себя).
pub unsafe fn send_ipi_all_others(vector: u8) {
    icr_wait();
    lapic_write(LAPIC_ICR_HIGH, 0);
    lapic_write(LAPIC_ICR_LOW, ICR_DEST_ALL_OTHER | ICR_FIXED | vector as u32);
    icr_wait();
}
