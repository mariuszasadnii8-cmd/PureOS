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
// boot_msg — пишет boot-сообщение в serial + UEFI ConOut
// ===================================================================

pub fn boot_msg(msg: &[u8]) {
    // Serial — всегда работает (даже до инициализации фреймбуфера).
    serial_puts(msg);
    // Экранный терминал — no-op, пока фреймбуфер не готов.
    crate::terminal::write(msg);
}
