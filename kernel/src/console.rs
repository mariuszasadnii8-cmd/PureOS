//! Консоль загрузки — serial + экранный текстовый терминал.
//!
//! boot_msg пишет в два канала:
//!   1. Serial (COM1, 0x3F8) — для QEMU -serial stdio
//!   2. Экранный терминал (`terminal`) — глифы прямо в GOP-фреймбуфер

// ===================================================================
// Serial (COM1, 0x3F8)
// ===================================================================

pub fn serial_putc(c: u8) {
    unsafe { core::arch::asm!("out dx, al", in("dx") 0x3F8u16, in("al") c); }
}

pub fn serial_puts(s: &[u8]) {
    for &c in s { serial_putc(c); }
}

pub fn serial_hex(val: u64) {
    let hex = b"0123456789abcdef";
    for shift in (0..64).step_by(4).rev() {
        serial_putc(hex[((val >> shift) & 0xF) as usize]);
    }
}

pub fn serial_dec(val: u64) {
    let mut buf = [0u8; 20];
    let mut i = buf.len();
    if val == 0 { serial_putc(b'0'); return; }
    let mut v = val;
    while v > 0 {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
    }
    for &c in &buf[i..] { serial_putc(c); }
}

// ===================================================================
// boot_msg — пишет boot-сообщение в serial + экран
// ===================================================================

pub fn boot_msg(msg: &[u8]) {
    serial_puts(msg);
    crate::terminal::write(msg);
}

// ===================================================================
// Boot progress bar — рисует прогресс прямо в фреймбуфер
// ===================================================================

/// Нарисовать прогресс-бар загрузки (вызывать после инициализации фреймбуфера).
/// progress: 0..100
pub fn boot_progress(progress: u32) {
    let p = progress.min(100);
    unsafe {
        use crate::framebuffer::{self, Rgb};
        let w = framebuffer::width();
        let h = framebuffer::height();
        if w == 0 || h == 0 { return; }

        // Позиция: внизу экрана, по центру
        let bar_w = w.min(600) as u32;    // макс 600 px
        let bar_h = 6u32;
        let bar_x = (w - bar_w) / 2;
        let bar_y = h - 40;

        // Фон прогресс-бара
        let bg = Rgb(30, 30, 40);
        let fill = Rgb(79, 183, 255); // акцентный голубой
        let border = Rgb(60, 60, 80);

        // Рамка
        for dx in 0..bar_w {
            framebuffer::put(bar_x + dx, bar_y, border);
            framebuffer::put(bar_x + dx, bar_y + bar_h - 1, border);
        }
        for dy in 0..bar_h {
            framebuffer::put(bar_x, bar_y + dy, border);
            framebuffer::put(bar_x + bar_w - 1, bar_y + dy, border);
        }
        // Фон
        framebuffer::fill_rect(bar_x + 1, bar_y + 1, bar_w - 2, bar_h - 2, bg);
        // Заливка прогресса
        if p > 0 {
            let fill_w = ((bar_w - 2) * p / 100).max(1);
            framebuffer::fill_rect(bar_x + 1, bar_y + 1, fill_w, bar_h - 2, fill);
        }
    }
}
