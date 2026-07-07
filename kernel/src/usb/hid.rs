//! USB HID Keyboard + Mouse driver (boot protocol).
//!
//! Zero-Alloc: статические структуры.

use crate::framebuffer::Rgb;
use crate::keyboard::KEY_META;

// ── Keyboard state ──

/// Последнее известное состояние кей-кодов (для детекта нажатия).
static mut LAST_KEYS: [u8; 6] = [0; 6];

/// Последние модификаторы (shift/ctrl/alt).
static mut LAST_MODS: u8 = 0;

/// Буфер прочитанных символов (кольцевой, 16 записей).
const KEY_BUF_SIZE: usize = 16;
static mut KEY_BUF: [u8; KEY_BUF_SIZE] = [0; KEY_BUF_SIZE];
static mut KEY_BUF_RD: usize = 0;
static mut KEY_BUF_WR: usize = 0;

// Key repeat state
static mut REPEAT_KEY: u8 = 0;
static mut REPEAT_MODS: u8 = 0;
static mut REPEAT_COUNTER: u32 = 0;
static mut REPEAT_ACTIVE: bool = false;
const REPEAT_DELAY: u32 = 30;   // ~300ms at ~10ms per poll
const REPEAT_RATE: u32 = 6;    // ~60ms between repeats

// ── Mouse state ──

/// Позиция курсора (пиксели, от 0).
static mut MOUSE_X: i32 = 0;
static mut MOUSE_Y: i32 = 0;

/// Состояние кнопок мыши (bit 0 = left, 1 = right, 2 = middle).
static mut MOUSE_BUTTONS: u8 = 0;

/// Cursor save area (для restore background при перемещении).
/// Включает 1-пиксельную рамку вокруг курсора для обводки (outline).
const CURSOR_W: usize = 12;
const CURSOR_H: usize = 16;
const CURSOR_BG_W: usize = CURSOR_W + 2; // 14
const CURSOR_BG_H: usize = CURSOR_H + 2; // 18
static mut CURSOR_BG: [u32; CURSOR_BG_W * CURSOR_BG_H] = [0; CURSOR_BG_W * CURSOR_BG_H];
static mut CURSOR_VISIBLE: bool = false;
static mut CURSOR_DIRTY: bool = true;

/// Позиция, где был сохранён фон (для корректного restore при движении).
static mut BG_SAVED_X: i32 = 0;
static mut BG_SAVED_Y: i32 = 0;

/// Растровый курсор-стрелка (1 = foreground, 0 = background).
/// 12×16 пикселей, упаковано побитово по строкам.
static CURSOR_BITMAP: [u16; CURSOR_H] = [
    0b1000_0000_0000,
    0b1100_0000_0000,
    0b1110_0000_0000,
    0b1111_0000_0000,
    0b1111_1000_0000,
    0b1111_1100_0000,
    0b1111_1110_0000,
    0b1111_1111_0000,
    0b1111_1111_1000,
    0b1111_1111_1100,
    0b1111_1111_1110,
    0b1111_1111_1111,
    0b1111_1110_0000,
    0b1100_1110_0000,
    0b0000_0110_0000,
    0b0000_0011_0000,
];

/// Обработать 8-байтный HID boot report.
pub unsafe fn process_report(report: &[u8; 8]) {
    if report.len() < 8 { return; }
    let mods = report[0];
    let keys = &report[2..8]; // 6-key rollover

    // Detect new key presses (keys that are in current but not in last)
    let mut any_new = false;
    for &k in keys {
        if k == 0 { continue; }
        if !last_has(k) {
            any_new = true;
            if let Some(ch) = keycode_to_ascii(k, mods) {
                push_key(ch);
            }
        }
    }

    // Key repeat logic
    let first_key = keys.iter().copied().find(|&k| k != 0).unwrap_or(0);
    if first_key == 0 {
        // No keys held — reset repeat
        REPEAT_KEY = 0;
        REPEAT_COUNTER = 0;
        REPEAT_ACTIVE = false;
    } else if any_new {
        // New key pressed — start repeat delay
        REPEAT_KEY = first_key;
        REPEAT_MODS = mods;
        REPEAT_COUNTER = 1;
        REPEAT_ACTIVE = false;
    } else if first_key == REPEAT_KEY {
        // Same key still held — advance counter
        REPEAT_COUNTER = REPEAT_COUNTER.wrapping_add(1);
        if !REPEAT_ACTIVE && REPEAT_COUNTER >= REPEAT_DELAY {
            REPEAT_ACTIVE = true;
            REPEAT_COUNTER = 0;
            // Emit first repeat
            if let Some(ch) = keycode_to_ascii(REPEAT_KEY, mods) {
                push_key(ch);
            }
        } else if REPEAT_ACTIVE && REPEAT_COUNTER >= REPEAT_RATE {
            REPEAT_COUNTER = 0;
            if let Some(ch) = keycode_to_ascii(REPEAT_KEY, mods) {
                push_key(ch);
            }
        }
    } else {
        // Different key now held (rollover) — restart repeat
        REPEAT_KEY = first_key;
        REPEAT_MODS = mods;
        REPEAT_COUNTER = 1;
        REPEAT_ACTIVE = false;
    }

    LAST_KEYS = [keys[0], keys[1], keys[2], keys[3], keys[4], keys[5]];
    LAST_MODS = mods;
}

/// Проверить, был ли код в предыдущем состоянии.
unsafe fn last_has(key: u8) -> bool {
    let ptr = &raw const LAST_KEYS;
    for i in 0..6 {
        if *ptr.cast::<u8>().add(i) == key { return true; }
    }
    false
}

/// Добавить символ в кольцевой буфер.
unsafe fn push_key(ch: u8) {
    let next = (KEY_BUF_WR + 1) % KEY_BUF_SIZE;
    if next != KEY_BUF_RD { // buffer not full
        KEY_BUF[KEY_BUF_WR] = ch;
        KEY_BUF_WR = next;
    }
}

/// Прочитать символ из буфера (если есть).
pub fn read_key() -> Option<u8> {
    unsafe {
        if KEY_BUF_RD == KEY_BUF_WR { return None; }
        let ch = KEY_BUF[KEY_BUF_RD];
        KEY_BUF_RD = (KEY_BUF_RD + 1) % KEY_BUF_SIZE;
        Some(ch)
    }
}

// ---------------------------------------------------------------------------
// USB HID keycode → ASCII
// ---------------------------------------------------------------------------

/// Преобразовать USB HID Usage ID (таблица 0x07) в ASCII.
/// Поддерживаются только буквы, цифры, Enter, Backspace, Tab, Escape, пробел
/// и основные знаки препинания (US layout).
fn keycode_to_ascii(key: u8, mods: u8) -> Option<u8> {
    let shift = mods & 0x22 != 0; // LShift (0x02) | RShift (0x20)
    let ctrl = mods & 0x11 != 0;  // LCtrl | RCtrl
    let alt = mods & 0x44 != 0;   // LAlt | RAlt

    if ctrl && alt { return None; } // ignore Alt+Ctrl combos

    match key {
        // Letters (a-z), same for shift → uppercase
        0x04 => Some(if shift { b'A' } else { b'a' }),
        0x05 => Some(if shift { b'B' } else { b'b' }),
        0x06 => Some(if shift { b'C' } else { b'c' }),
        0x07 => Some(if shift { b'D' } else { b'd' }),
        0x08 => Some(if shift { b'E' } else { b'e' }),
        0x09 => Some(if shift { b'F' } else { b'f' }),
        0x0A => Some(if shift { b'G' } else { b'g' }),
        0x0B => Some(if shift { b'H' } else { b'h' }),
        0x0C => Some(if shift { b'I' } else { b'i' }),
        0x0D => Some(if shift { b'J' } else { b'j' }),
        0x0E => Some(if shift { b'K' } else { b'k' }),
        0x0F => Some(if shift { b'L' } else { b'l' }),
        0x10 => Some(if shift { b'M' } else { b'm' }),
        0x11 => Some(if shift { b'N' } else { b'n' }),
        0x12 => Some(if shift { b'O' } else { b'o' }),
        0x13 => Some(if shift { b'P' } else { b'p' }),
        0x14 => Some(if shift { b'Q' } else { b'q' }),
        0x15 => Some(if shift { b'R' } else { b'r' }),
        0x16 => Some(if shift { b'S' } else { b's' }),
        0x17 => Some(if shift { b'T' } else { b't' }),
        0x18 => Some(if shift { b'U' } else { b'u' }),
        0x19 => Some(if shift { b'V' } else { b'v' }),
        0x1A => Some(if shift { b'W' } else { b'w' }),
        0x1B => Some(if shift { b'X' } else { b'x' }),
        0x1C => Some(if shift { b'Y' } else { b'y' }),
        0x1D => Some(if shift { b'Z' } else { b'z' }),

        // Numbers (top row)
        0x1E => Some(if shift { b'!' } else { b'1' }),
        0x1F => Some(if shift { b'@' } else { b'2' }),
        0x20 => Some(if shift { b'#' } else { b'3' }),
        0x21 => Some(if shift { b'$' } else { b'4' }),
        0x22 => Some(if shift { b'%' } else { b'5' }),
        0x23 => Some(if shift { b'^' } else { b'6' }),
        0x24 => Some(if shift { b'&' } else { b'7' }),
        0x25 => Some(if shift { b'*' } else { b'8' }),
        0x26 => Some(if shift { b'(' } else { b'9' }),
        0x27 => Some(if shift { b')' } else { b'0' }),

        // Special keys
        0x28 => Some(b'\n'),      // Enter
        0x29 => Some(0x1B),       // Escape
        0x2A => Some(0x08),       // Backspace
        0x2B => Some(b'\t'),      // Tab
        0x2C => Some(b' '),       // Space

        // Punctuation: US layout
        0x2D => Some(if shift { b'_' } else { b'-' }),
        0x2E => Some(if shift { b'+' } else { b'=' }),
        0x2F => Some(if shift { b'{' } else { b'[' }),
        0x30 => Some(if shift { b'}' } else { b']' }),
        0x31 => Some(if shift { b'|' } else { b'\\' }),
        0x32 => Some(if shift { b':' } else { b';' }),
        0x33 => Some(if shift { b'"' } else { b'\'' }),
        0x34 => Some(if shift { b'~' } else { b'`' }),
        0x35 => Some(if shift { b'<' } else { b',' }),
        0x36 => Some(if shift { b'>' } else { b'.' }),
        0x37 => Some(if shift { b'?' } else { b'/' }),

        // Keypad
        0x59 => Some(if shift { b'1' } else { b'1' }),
        0x5A => Some(if shift { b'2' } else { b'2' }),
        0x5B => Some(if shift { b'3' } else { b'3' }),
        0x5C => Some(if shift { b'4' } else { b'4' }),
        0x5D => Some(if shift { b'5' } else { b'5' }),
        0x5E => Some(if shift { b'6' } else { b'6' }),
        0x5F => Some(if shift { b'7' } else { b'7' }),
        0x60 => Some(if shift { b'8' } else { b'8' }),
        0x61 => Some(if shift { b'9' } else { b'9' }),
        0x62 => Some(if shift { b'0' } else { b'0' }),

                // Meta/Windows key (Left GUI / Right GUI)
                0xE3 | 0xE7 => Some(KEY_META),

                _ => None,
            }
        }

// ═══════════════════════════════════════════════════════════════════════════════
// Mouse
// ═══════════════════════════════════════════════════════════════════════════════

/// Обработать 3-байтный HID boot mouse report.
/// Byte 0: buttons, Byte 1: X delta, Byte 2: Y delta (signed).
pub unsafe fn process_mouse_report(report: &[u8; 3]) {
    let buttons = report[0] & 0x07;
    let sens = crate::config::get_mouse_sensitivity();
    let dx = (report[1] as i8 as i32) * sens as i32 / 5;
    let dy = (report[2] as i8 as i32) * sens as i32 / 5;

    MOUSE_BUTTONS = buttons;

    let fb_w = crate::framebuffer::width() as i32;
    let fb_h = crate::framebuffer::height() as i32;
    let new_x = MOUSE_X.wrapping_add(dx).max(0).min(fb_w - 1);
    let new_y = MOUSE_Y.wrapping_add(dy).max(0).min(fb_h - 1);

    if new_x != MOUSE_X || new_y != MOUSE_Y {
        MOUSE_X = new_x;
        MOUSE_Y = new_y;
        CURSOR_DIRTY = true;
    }
}

/// Сохранить фон под курсором (включая 1px под обводку).
pub unsafe fn save_cursor_bg() {
    BG_SAVED_X = MOUSE_X;
    BG_SAVED_Y = MOUSE_Y;
    let x0 = MOUSE_X - 1;
    let y0 = MOUSE_Y - 1;
    let bw = crate::framebuffer::width() as i32;
    let bh = crate::framebuffer::height() as i32;
    for row in 0..CURSOR_BG_H {
        for col in 0..CURSOR_BG_W {
            let fx = x0 + col as i32;
            let fy = y0 + row as i32;
            let packed = if fx >= 0 && fy >= 0 && fx < bw && fy < bh {
                match crate::framebuffer::get(fx as u32, fy as u32) {
                    Some(Rgb(r, g, b)) => (r as u32) | ((g as u32) << 8) | ((b as u32) << 16),
                    None => 0,
                }
            } else { 0 };
            CURSOR_BG[row * CURSOR_BG_W + col] = packed;
        }
    }
}

/// Восстановить фон под курсором (включая 1px под обводку).
/// Использует позицию, где был сохранён фон (BG_SAVED_X/Y), а не текущую.
pub unsafe fn restore_cursor_bg() {
    let x0 = BG_SAVED_X - 1;
    let y0 = BG_SAVED_Y - 1;
    for row in 0..CURSOR_BG_H {
        for col in 0..CURSOR_BG_W {
            let fx = x0 + col as i32;
            let fy = y0 + row as i32;
            if fx < 0 || fy < 0 { continue; }
            let packed = CURSOR_BG[row * CURSOR_BG_W + col];
            let r = (packed & 0xFF) as u8;
            let g = ((packed >> 8) & 0xFF) as u8;
            let b = ((packed >> 16) & 0xFF) as u8;
            crate::framebuffer::put(fx as u32, fy as u32, Rgb(r, g, b));
        }
    }
}

/// Нарисовать курсор на его текущей позиции.
pub unsafe fn draw_cursor() {
    let _x = MOUSE_X;
    let _y = MOUSE_Y;
    let fg = Rgb(255, 255, 255); // белый
    let out = Rgb(0, 0, 0);      // чёрный обвод (инверт)

    for row in 0..CURSOR_H {
        let bits = CURSOR_BITMAP[row];
        for col in 0..CURSOR_W {
            if bits & (1 << (CURSOR_W - 1 - col)) != 0 {
                let px = MOUSE_X + col as i32;
                let py = MOUSE_Y + row as i32;
                if px >= 0 && py >= 0 {
                    crate::framebuffer::put(px as u32, py as u32, fg);
                }
            }
        }
    }
    // Обводка контура (чёрная рамка)
    for row in 0..CURSOR_H {
        let bits = CURSOR_BITMAP[row];
        for col in 0..CURSOR_W {
            if bits & (1 << (CURSOR_W - 1 - col)) == 0 { continue; }
            // Check neighbours for background
            for (ndx, ndy) in &[(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                let nx = col as i32 + ndx;
                let ny = row as i32 + ndy;
                if nx < 0 || nx >= CURSOR_W as i32 || ny < 0 || ny >= CURSOR_H as i32 { continue; }
                if CURSOR_BITMAP[ny as usize] & (1 << (CURSOR_W - 1 - nx as usize)) == 0 {
                    let px = MOUSE_X + col as i32 + ndx;
                    let py = MOUSE_Y + row as i32 + ndy;
                    if px >= 0 && py >= 0 {
                        crate::framebuffer::put(px as u32, py as u32, out);
                    }
                }
            }
        }
    }
    CURSOR_VISIBLE = true;
    CURSOR_DIRTY = false;
}

/// Hides the cursor (restore background).
pub unsafe fn hide_cursor() {
    if CURSOR_VISIBLE {
        restore_cursor_bg();
        CURSOR_VISIBLE = false;
    }
}

/// Shows the cursor (save bg + draw).
pub unsafe fn show_cursor() {
    if !CURSOR_VISIBLE {
        save_cursor_bg();
        draw_cursor();
        CURSOR_VISIBLE = true;
    }
}

/// Получить позицию курсора.
pub fn mouse_pos() -> (i32, i32) {
    unsafe { (MOUSE_X, MOUSE_Y) }
}

/// Получить состояние кнопок мыши.
pub fn mouse_buttons() -> u8 {
    unsafe { MOUSE_BUTTONS }
}

/// Установить позицию (вызывается из PS/2 мыши/тачпада).
pub unsafe fn mouse_set_pos(x: i32, y: i32) {
    MOUSE_X = x;
    MOUSE_Y = y;
    CURSOR_DIRTY = true;
}

/// Установить кнопки (вызывается из PS/2 мыши/тачпада).
pub unsafe fn mouse_set_buttons(buttons: u8) {
    MOUSE_BUTTONS = buttons;
}

/// Обновить курсор (вызывается из shell loop и desktop loop).
/// Всегда перерисовывает курсор, если он должен быть видим
/// (даже после mouse_hide, когда CURSOR_DIRTY мог сброситься).
pub unsafe fn update_cursor() {
    if CURSOR_DIRTY || !CURSOR_VISIBLE {
        if CURSOR_VISIBLE {
            restore_cursor_bg();
        }
        save_cursor_bg();
        draw_cursor();
    }
}

/// Сбросить курсор в центр экрана при инициализации.
pub unsafe fn init_mouse() {
    MOUSE_X = (crate::framebuffer::width() as i32) / 2;
    MOUSE_Y = (crate::framebuffer::height() as i32) / 2;
    MOUSE_BUTTONS = 0;
    CURSOR_VISIBLE = false;
    CURSOR_DIRTY = true;
    BG_SAVED_X = MOUSE_X;
    BG_SAVED_Y = MOUSE_Y;
}
