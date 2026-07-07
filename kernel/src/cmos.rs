//! CMOS/RTC драйвер — чтение реального времени через порты 0x70/0x71.
//!
//! Регистры CMOS:
//!   0x00 — секунды
//!   0x02 — минуты
//!   0x04 — часы
//!   0x06 — день недели
//!   0x07 — день месяца
//!   0x08 — месяц
//!   0x09 — год (0..99)
//!   0x0A — Status Register A (UIP бит)
//!   0x0B — Status Register B (DM=1 → binary, 24/12=1 → 24hr)

use crate::cpu;

/// Прочитать байт из CMOS по индексу.
unsafe fn cmos_read(idx: u8) -> u8 {
    cpu::outb(0x70, idx);
    cpu::inb(0x71)
}

/// Структура с текущим временем.
#[derive(Clone, Copy)]
pub struct RtcTime {
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub day: u8,
    pub month: u8,
    pub year: u16, // полный год, напр. 2026
}

/// Преобразовать BCD в binary.
fn bcd2bin(bcd: u8) -> u8 {
    (bcd & 0x0F) + ((bcd >> 4) * 10)
}

/// Прочитать время из CMOS.
pub unsafe fn read_rtc() -> RtcTime {
    // Ждём, пока CMOS не в процессе обновления (UIP=0)
    let mut timeout = 1000;
    while (cmos_read(0x0A) & 0x80) != 0 && timeout > 0 {
        timeout -= 1;
    }

    let status_b = cmos_read(0x0B);
    let is_binary = (status_b & 0x04) != 0;
    let is_24h = (status_b & 0x02) != 0;

    // Читаем все регистры (дважды для консистентности)
    let mut sec = cmos_read(0x00);
    let mut min = cmos_read(0x02);
    let mut hour = cmos_read(0x04);
    let mut day = cmos_read(0x07);
    let mut month = cmos_read(0x08);
    let mut year = cmos_read(0x09);

    // Повторная проверка на обновление
    if cmos_read(0x0A) & 0x80 != 0 {
        timeout = 1000;
        while (cmos_read(0x0A) & 0x80) != 0 && timeout > 0 {
            timeout -= 1;
        }
        sec = cmos_read(0x00);
        min = cmos_read(0x02);
        hour = cmos_read(0x04);
        day = cmos_read(0x07);
        month = cmos_read(0x08);
        year = cmos_read(0x09);
    }

    // Convert to binary
    if !is_binary {
        sec = bcd2bin(sec);
        min = bcd2bin(min);
        hour = bcd2bin(hour);
        day = bcd2bin(day);
        month = bcd2bin(month);
        year = bcd2bin(year);
    }

    // Convert 12h → 24h
    if !is_24h {
        let pm = hour & 0x80 != 0;
        hour = hour & 0x7F;
        if pm {
            hour = if hour == 12 { 12 } else { (hour % 12) + 12 };
        } else {
            hour = if hour == 12 { 0 } else { hour % 12 };
        }
    }

    RtcTime {
        hour,
        minute: min,
        second: sec,
        day,
        month,
        year: 2000 + year as u16,
    }
}

/// Прочитать только часы и минуты (быстрый вариант).
pub unsafe fn read_time() -> (u8, u8) {
    let mut timeout = 1000;
    while (cmos_read(0x0A) & 0x80) != 0 && timeout > 0 {
        timeout -= 1;
    }
    let is_binary = (cmos_read(0x0B) & 0x04) != 0;
    let is_24h = (cmos_read(0x0B) & 0x02) != 0;

    let mut hour = cmos_read(0x04);
    let mut min = cmos_read(0x02);

    if !is_binary {
        hour = bcd2bin(hour);
        min = bcd2bin(min);
    }
    if !is_24h {
        let pm = hour & 0x80 != 0;
        hour = hour & 0x7F;
        if pm {
            hour = if hour == 12 { 12 } else { (hour % 12) + 12 };
        } else {
            hour = if hour == 12 { 0 } else { hour % 12 };
        }
    }
    (hour, min)
}

/// Отформатировать время как строку "HH:MM".
pub fn format_time(hour: u8, min: u8) -> [u8; 5] {
    [
        b'0' + (hour / 10),
        b'0' + (hour % 10),
        b':',
        b'0' + (min / 10),
        b'0' + (min % 10),
    ]
}
