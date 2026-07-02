//! Текстовый терминал поверх GOP-фреймбуфера (own text-mode console).
//!
//! Раньше вывод шёл через UEFI ConOut — но OVMF в графическом режиме GOP не
//! рендерит консольный текст на экран, поэтому на дисплее оставался градиент
//! загрузчика. Теперь рисуем глифы 8x8 сами прямо в линейный фреймбуфер:
//! сетка ячеек, курсор, прокрутка, цвета. UEFI нужен только для ввода
//! (Simple Text Input) и Runtime Services (reboot).
//!
//! Zero-Alloc: всё состояние — статические переменные фиксированного размера.

use crate::framebuffer::{self, Rgb};

/// Масштаб глифа: шрифт 8x8 → крупная и читаемая ячейка.
const SCALE: u32 = 3;
const CELL: u32 = 8 * SCALE;

// Палитра терминала.
const BG: Rgb = Rgb(0x06, 0x0b, 0x10);
const FG: Rgb = Rgb(0xb8, 0xf2, 0x92);
const ACCENT: Rgb = Rgb(0x4f, 0xb7, 0xff);

static mut CUR_COL: u32 = 0;
static mut CUR_ROW: u32 = 0;
static mut FG_COLOR: Rgb = FG;
static mut BG_COLOR: Rgb = BG;
static mut READY: bool = false;

#[inline(always)]
fn cols() -> u32 {
    let w = framebuffer::width();
    if w == 0 { 0 } else { w / CELL }
}

#[inline(always)]
fn rows() -> u32 {
    let h = framebuffer::height();
    if h == 0 { 0 } else { h / CELL }
}

/// Инициализировать терминал: очистить экран, сбросить курсор и цвета.
pub fn init() {
    unsafe {
        FG_COLOR = FG;
        BG_COLOR = BG;
        CUR_COL = 0;
        CUR_ROW = 0;
        READY = framebuffer::width() != 0 && framebuffer::height() != 0;
    }
    clear();
}

/// Готов ли экранный терминал (есть фреймбуфер).
pub fn is_init() -> bool {
    unsafe { READY }
}

/// Очистить экран фоном и увести курсор в левый верхний угол.
pub fn clear() {
    unsafe {
        framebuffer::clear(BG_COLOR);
        CUR_COL = 0;
        CUR_ROW = 0;
    }
}

/// Нарисовать красивый boot-баннер и промпт в консоли.
pub fn draw_boot_banner() {
    if !is_init() {
        init();
    }

    clear();
    set_colors(ACCENT, BG);
    write(b"  PUREOS\n");
    set_colors(FG, BG);
    write(b"  Crystal Kernel\n");
    write(b"  developer console\n\n");
    set_colors(ACCENT, BG);
    write(b"  ready\n");
    set_colors(FG, BG);
    write(b"\n");
    write(b"pure/main# ");
}

/// Установить цвета (fg/bg) терминала.
pub fn set_colors(fg: Rgb, bg: Rgb) {
    unsafe {
        FG_COLOR = fg;
        BG_COLOR = bg;
    }
}

/// Вывести ASCII-строку.
pub fn write(s: &[u8]) {
    for &ch in s {
        putchar(ch);
    }
}

/// Вывести один символ, обрабатывая управляющие коды.
pub fn putchar(ch: u8) {
    if !unsafe { READY } {
        return;
    }
    match ch {
        b'\n' => unsafe { newline() },
        b'\r' => unsafe { CUR_COL = 0 },
        b'\t' => {
            // Табуляция до следующей границы в 4 ячейки.
            let next = (unsafe { CUR_COL } / 4 + 1) * 4;
            let limit = cols();
            unsafe {
                while CUR_COL < next && CUR_COL < limit {
                    put_cell(b' ');
                    CUR_COL += 1;
                }
            }
            if unsafe { CUR_COL } >= limit {
                unsafe { newline() };
            }
        }
        0x08 | 0x7F => unsafe { backspace() },
        0x20..=0x7E => {
            if unsafe { CUR_COL >= cols() } {
                unsafe { newline() };
            }
            unsafe { put_cell(ch); }
            unsafe { CUR_COL += 1; }
        },
        _ => {}
    }
}

/// Нарисовать глиф текущим цветом в ячейке курсора (фон под ним — bg).
unsafe fn put_cell(ch: u8) {
    let x = CUR_COL * CELL;
    let y = CUR_ROW * CELL;
    // Фон ячейки + глиф поверх.
    framebuffer::fill_rect(x, y, CELL, CELL, BG_COLOR);
    framebuffer::draw_char(x, y, ch, FG_COLOR, SCALE);
}

unsafe fn newline() {
    CUR_COL = 0;
    CUR_ROW += 1;
    if CUR_ROW >= rows() {
        scroll();
        CUR_ROW = rows().saturating_sub(1);
    }
}

unsafe fn backspace() {
    if CUR_COL > 0 {
        CUR_COL -= 1;
    } else if CUR_ROW > 0 {
        CUR_ROW -= 1;
        CUR_COL = cols().saturating_sub(1);
    }
    // Стереть ячейку под курсором.
    let x = CUR_COL * CELL;
    let y = CUR_ROW * CELL;
    framebuffer::fill_rect(x, y, CELL, CELL, BG_COLOR);
}

/// Прокрутить экран на одну текстовую строку вверх (сдвиг пикселей + очистка
/// нижней строки). Быстрее полной перерисовки: копируем линейную память кадра.
unsafe fn scroll() {
    let h = framebuffer::height();
    let w = framebuffer::width();
    let stride = framebuffer::stride();
    let base = framebuffer::base();
    if base == 0 || h <= CELL {
        return;
    }

    let shift = CELL as u64 * stride as u64; // сдвиг в пикселях (u32-элементах)
    let total = h as u64 * stride as u64;
    let count = (total - shift) as usize;

    // memmove вверх (регионы перекрываются) — core::ptr::copy это учитывает.
    core::ptr::copy(
        (base + shift * 4) as *const u32,
        base as *mut u32,
        count,
    );

    // Очистить освободившуюся нижнюю текстовую строку.
    framebuffer::fill_rect(0, h - CELL, w, CELL, BG_COLOR);
}

/// Вывести десятичное число.
pub fn write_num(val: u64) {
    if val == 0 {
        putchar(b'0');
        return;
    }
    let mut buf = [0u8; 20];
    let mut i = buf.len();
    let mut v = val;
    while v > 0 {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
    }
    write(&buf[i..]);
}

/// Вывести hex-число (0x...).
pub fn write_hex(val: u64) {
    let hex = b"0123456789abcdef";
    write(b"0x");
    for shift in (0..64).step_by(4).rev() {
        putchar(hex[((val >> shift) & 0xF) as usize]);
    }
}
