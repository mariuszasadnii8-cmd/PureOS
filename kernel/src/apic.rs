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

use crate::cpu;

// LAPIC базовый адрес (обычно 0xFEE00000 для x86)
const LAPIC_BASE: u64 = 0xFEE0_0000;
// MSR для детекции x2APIC
const IA32_APIC_BASE: u32 = 0x1B;
const X2APIC_ENABLE: u64 = 1 << 10;
const X2APIC_MSR_BASE: u32 = 0x800;

// Смещения регистров LAPIC (в байтах)
const LAPIC_ID: u64 = 0x020;
const LAPIC_VERSION: u64 = 0x030;
const LAPIC_EOI: u64 = 0x0B0;
const LAPIC_SPURIOUS: u64 = 0x0F0;
const LAPIC_ICR_LOW: u64 = 0x300;
const LAPIC_ICR_HIGH: u64 = 0x310;
pub const LAPIC_LVT_TIMER: u64 = 0x320;
pub const LAPIC_INIT_COUNT: u64 = 0x380;
pub const LAPIC_CURRENT_COUNT: u64 = 0x390;
pub const LAPIC_DIVIDE_CONFIG: u64 = 0x3E0;

const TIMER_PERIODIC: u32 = 0x20000;

// ICR delivery mode (биты 8-10 в ICR_LOW)
const ICR_FIXED: u32 = 0 << 8;
const ICR_INIT: u32 = 5 << 8;
const ICR_SIPI: u32 = 6 << 8;

// ICR destination shorthand (биты 18-19)
const ICR_DEST_ALL_EXCL_SELF: u32 = 3 << 18;

const ICR_ASSERT: u32 = 1 << 14;
const ICR_TRIGGER_LEVEL: u32 = 1 << 15;

/// Вектор прерывания таймера.
pub const TIMER_VECTOR: u8 = 0x20;

/// Вектор IPI (SMP wakeup / work dispatch).
pub const IPI_WAKE_VECTOR: u8 = 0x21;

static mut APIC_READY: bool = false;

/// Калиброванное значение INIT_COUNT для ~10ms.
static mut CALIBRATED_COUNT: u32 = 10_000_000;

/// Режим x2APIC (MSR-доступ вместо memory-mapped).
static mut X2APIC_MODE: bool = false;

#[inline(always)]
pub unsafe fn lapic_read(reg: u64) -> u32 {
    if X2APIC_MODE {
        let msr = (X2APIC_MSR_BASE + (reg as u32 / 16)) as u32;
        cpu::rdmsr(msr) as u32
    } else {
        read_volatile((LAPIC_BASE + reg) as *const u32)
    }
}

#[inline(always)]
pub unsafe fn lapic_write(reg: u64, val: u32) {
    if X2APIC_MODE {
        let msr = (X2APIC_MSR_BASE + (reg as u32 / 16)) as u32;
        cpu::wrmsr(msr, val as u64);
    } else {
        write_volatile((LAPIC_BASE + reg) as *mut u32, val);
    }
}

/// Запись ICR (Interrupt Command Register) — разный формат в xAPIC и x2APIC.
/// В xAPIC: split ICR_HIGH (dest << 24) + ICR_LOW, с ожиданием busy.
/// В x2APIC: единый 64-битный MSR (dest в upper 32 bits), busy бита нет.
#[inline(always)]
unsafe fn write_icr(low: u32, dest_apic_id: u32) {
    if X2APIC_MODE {
        cpu::wrmsr(
            X2APIC_MSR_BASE + 0x30,
            ((dest_apic_id as u64) << 32) | (low as u64),
        );
    } else {
        icr_wait();
        write_volatile((LAPIC_BASE + LAPIC_ICR_HIGH) as *mut u32, dest_apic_id << 24);
        write_volatile((LAPIC_BASE + LAPIC_ICR_LOW) as *mut u32, low);
        icr_wait();
    }
}

/// Прочитать APIC ID текущего процессора (BSP или AP).
pub unsafe fn lapic_id() -> u32 {
    let id = lapic_read(LAPIC_ID);
    if X2APIC_MODE {
        id // x2APIC: полный 32-битный ID
    } else {
        (id >> 24) & 0xFF // xAPIC: ID в битах 31:24
    }
}

/// Инициализировать LAPIC: включить, настроить таймер.
/// НЕ ИСПОЛЬЗУЕТСЯ: ядро использует кооперативный планировщик.
pub unsafe fn init() {
    // APIC timer отключен для избежания конфликтов с UEFI
    APIC_READY = false;
}

/// Калибровать APIC-таймер, измерив его скорость относительно TSC.
/// После вызова `CALIBRATED_COUNT` устанавливается так, чтобы таймер
/// генерировал прерывание каждые ~10ms.
pub unsafe fn calibrate() {
    let divider = 0x0B; // divisor = 1 (0x0B = divide by 1)

    // 1. Включить таймер в однократном режиме с макс. начальным счётом
    lapic_write(LAPIC_DIVIDE_CONFIG, divider);
    // Однократный режим (без TIMER_PERIODIC)
    lapic_write(LAPIC_LVT_TIMER, TIMER_VECTOR as u32);
    lapic_write(LAPIC_INIT_COUNT, 0xFFFF_FFFF);

    // 2. Измерить APIC-счёт за интервал TSC
    //    Используем ~1 ms spin-loop как грубый ориентир
    let start_count = lapic_read(LAPIC_CURRENT_COUNT);

    // 3. Задержка: простой spin-loop
    //    ~1M итераций — примерно 1ms на 1GHz CPU
    for _ in 0..500_000 {
        core::hint::spin_loop();
    }

    let end_count = lapic_read(LAPIC_CURRENT_COUNT);

    // 4. Сколько APIC-тиков прошло
    let elapsed = if start_count > end_count {
        start_count - end_count
    } else {
        0xFFFF_FFFF - end_count + start_count
    };

    if elapsed > 1000 {
        // 5. Экстраполировать: target = тики за 10ms
        //    Мы ждали ~0.5ms (500,000 итераций), так:
        //    тиков за 10ms = elapsed * 20
        let target = elapsed * 20;

        // Ограничим разумными пределами
        if target > 100_000 && target < 1_000_000_000 {
            CALIBRATED_COUNT = target;
        }
    }

    // 6. Выключить таймер (будет включён при init_ap)
    lapic_write(LAPIC_LVT_TIMER, 0x10000); // masked
}

/// Вернуть калиброванное значение INIT_COUNT.
pub fn calibrated_count() -> u32 {
    unsafe { CALIBRATED_COUNT }
}

/// Инициализировать LAPIC на AP (только таймер, spurious уже включён ядром).
pub unsafe fn init_ap() {
    lapic_write(LAPIC_DIVIDE_CONFIG, 0x0B);
    lapic_write(LAPIC_LVT_TIMER, TIMER_PERIODIC | TIMER_VECTOR as u32);
    lapic_write(LAPIC_INIT_COUNT, CALIBRATED_COUNT);
}

/// Сигнал конца прерывания (EOI).
pub unsafe fn eoi() {
    lapic_write(LAPIC_EOI, 0);
}

/// Готова ли APIC.
pub fn is_ready() -> bool {
    unsafe { APIC_READY }
}

/// Детектировать x2APIC-режим: читать IA32_APIC_BASE MSR, бит 10.
/// Вызвать ДО любых lapic_read/lapic_write (сразу после `cli`).
pub unsafe fn detect_x2apic() {
    let apic_base = cpu::rdmsr(IA32_APIC_BASE);
    X2APIC_MODE = (apic_base & X2APIC_ENABLE) != 0;
}

/// Вернуть true, если APIC в x2APIC-режиме.
pub fn x2apic_enabled() -> bool {
    unsafe { X2APIC_MODE }
}

/// Обработчик тика таймера. Вызывается из IDT.
pub unsafe fn timer_tick() {
    if !APIC_READY { return; }
    crate::syscall::timer_tick_handler();
    eoi();
}

/// Ожидать готовность ICR (предыдущий IPI завершён).
/// В x2APIC busy-бита нет — возврат сразу.
unsafe fn icr_wait() {
    if !X2APIC_MODE {
        loop {
            if lapic_read(LAPIC_ICR_LOW) & (1 << 12) == 0 {
                break;
            }
            core::hint::spin_loop();
        }
    }
}

/// Отправить IPI конкретному APIC ID (фиксированный вектор).
pub unsafe fn send_ipi(apic_id: u32, vector: u8) {
    write_icr(ICR_FIXED | ICR_ASSERT | vector as u32, apic_id);
}

/// Отправить INIT IPI конкретному APIC ID (для AP wakeup).
pub unsafe fn send_init_ipi(apic_id: u32) {
    write_icr(ICR_INIT | ICR_ASSERT | ICR_TRIGGER_LEVEL, apic_id);
}

/// Отправить INIT DE-ASSERT конкретному APIC ID.
pub unsafe fn send_init_deassert(apic_id: u32) {
    write_icr(ICR_INIT | ICR_TRIGGER_LEVEL, apic_id);
}

/// Отправить SIPI конкретному APIC ID с вектором (адрес = vector * 0x1000).
pub unsafe fn send_startup_ipi(apic_id: u32, vector: u8) {
    write_icr(ICR_SIPI | vector as u32, apic_id);
}

/// Отправить IPI всем остальным процессорам (broadcast, кроме себя).
pub unsafe fn send_ipi_all_others(vector: u8) {
    write_icr(ICR_DEST_ALL_EXCL_SELF | ICR_FIXED | vector as u32, 0);
}

/// Отправить INIT IPI всем остальным процессорам (broadcast, кроме себя).
pub unsafe fn send_init_ipi_all_others() {
    write_icr(ICR_DEST_ALL_EXCL_SELF | ICR_INIT | ICR_ASSERT | ICR_TRIGGER_LEVEL, 0);
}

/// Отправить INIT DE-ASSERT всем остальным (очистка состояния перед assert).
/// Нужно на некоторых real-хардварных платформах — без этого INIT может не сработать.
pub unsafe fn send_init_deassert_all_others() {
    write_icr(ICR_DEST_ALL_EXCL_SELF | ICR_INIT | ICR_TRIGGER_LEVEL, 0);
}

/// Отправить SIPI всем остальным процессорам (broadcast, кроме себя).
pub unsafe fn send_startup_ipi_all_others(vector: u8) {
    write_icr(ICR_DEST_ALL_EXCL_SELF | ICR_SIPI | vector as u32, 0);
}
