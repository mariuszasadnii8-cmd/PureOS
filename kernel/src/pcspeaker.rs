//! PC Speaker драйвер — простой звуковой сигнал через PIT + порт 0x61.
//!
//! Использует PIT (Programmable Interval Timer) канал 2 и PC Speaker gate.
//! Порты:
//!   0x43 — PIT mode/command register
//!   0x42 — PIT channel 2 data port
//!   0x61 — PC Speaker control port (bit 0 = timer2 gate, bit 1 = speaker data)

use crate::cpu;

/// Воспроизвести звук заданной частоты (Гц).
/// Блокирует выполнение на ~`ms` миллисекунд через spin-loop.
pub unsafe fn beep(freq_hz: u32, ms: u32) {
    if freq_hz == 0 { return; }

    // PIT частота 1.193182 MHz
    let divisor = (1193182u32 / freq_hz.max(20).min(20000)) as u16;

    // Запрограммировать PIT channel 2: mode 3 (square wave)
    cpu::outb(0x43, 0xB6); // channel 2, mode 3, lobyte/hibyte

    cpu::outb(0x42, (divisor & 0xFF) as u8);
    cpu::outb(0x42, (divisor >> 8) as u8);

    // Включить PC Speaker (port 0x61, set bits 0 and 1)
    let current = cpu::inb(0x61);
    cpu::outb(0x61, current | 0x03);

    // Spin-loop delay (грубый)
    for _ in 0..(ms * 2000) {
        core::hint::spin_loop();
    }

    // Выключить
    cpu::outb(0x61, current & !0x03);
}

/// Воспроизвести сигнал с фиксированной частотой 800 Гц.
pub unsafe fn beep_short() {
    beep(800, 100);
}

/// Воспроизвести сигнал ошибки (два коротких).
pub unsafe fn beep_error() {
    beep(400, 150);
    beep(300, 150);
}

/// Воспроизвести мелодию (массив пар частота_Гц, длительность_мс).
/// Завершается нулевой частотой.
pub unsafe fn play(melody: &[(u32, u32)]) {
    for &(freq, dur) in melody {
        if freq == 0 { break; }
        beep(freq, dur);
    }
}

/// Проиграть простую мелодию при загрузке/запуске.
pub unsafe fn boot_jingle() {
    const C5: u32 = 523;
    const E5: u32 = 659;
    const G5: u32 = 784;
    const C6: u32 = 1047;
    play(&[(C5, 100), (E5, 100), (G5, 100), (C6, 200), (0, 0)]);
}

/// Выключить PC Speaker (без блокировки).
pub unsafe fn off() {
    let current = cpu::inb(0x61);
    cpu::outb(0x61, current & !0x03);
}
