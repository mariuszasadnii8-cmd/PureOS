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
use crate::font::{self, FontId};

/// Масштаб глифа — переменная, устанавливается из конфига при инициализации.
static mut SCALE: u32 = 2;
static mut CELL: u32 = 16;
static mut SELECTED_FONT: FontId = FontId::Compact;

/// Получить выбранный шрифт из конфига.
unsafe fn apply_font_config() {
    let cfg = crate::config::get_config();
    let idx = cfg.selected_font;
    SELECTED_FONT = if idx < font::FONT_COUNT as u32 {
        match idx {
            0 => FontId::Compact,
            1 => FontId::Bold,
            2 => FontId::Italic,
            3 => FontId::Serif,
            4 => FontId::Outline,
            5 => FontId::Tall,
            6 => FontId::Vga,
            _ => FontId::Wide,
        }
    } else {
        FontId::Compact
    };
    SCALE = cfg.font_scale.clamp(1, 4);
    let fw = font::font_width(SELECTED_FONT);
    CELL = fw * SCALE;
}

// Палитра терминала.
const BG: Rgb = Rgb(0x06, 0x0b, 0x10);
const FG: Rgb = Rgb(0xb8, 0xf2, 0x92);
const ACCENT: Rgb = Rgb(0x4f, 0xb7, 0xff);

static mut CUR_COL: u32 = 0;
static mut CUR_ROW: u32 = 0;
static mut FG_COLOR: Rgb = FG;
static mut BG_COLOR: Rgb = BG;
static mut READY: bool = false;

/// Режим стеклянного терминала (glassmorphism).
/// 0 = выкл (сплошной фон), 1 = лёгкий, 2 = средний, 3 = сильный
static mut GLASS_MODE: u32 = 0;

#[inline(always)]
fn cols() -> u32 {
    let w = framebuffer::width();
    if w == 0 { 0 } else { w / unsafe { CELL } }
}

#[inline(always)]
fn rows() -> u32 {
    let h = framebuffer::height();
    if h == 0 { 0 } else { h / unsafe { CELL } }
}

/// Инициализировать терминал: очистить экран, сбросить курсор и цвета.
pub fn init() {
    unsafe {
        apply_font_config();
        // Загрузить цвета из конфигурации (если пользователь менял).
        let cfg = crate::config::get_config();
        FG_COLOR = Rgb(cfg.terminal_colors.foreground_r,
                       cfg.terminal_colors.foreground_g,
                       cfg.terminal_colors.foreground_b);
        BG_COLOR = Rgb(cfg.terminal_colors.background_r,
                       cfg.terminal_colors.background_g,
                       cfg.terminal_colors.background_b);
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
        let glass = GLASS_MODE;
        if glass > 0 {
            // Glass: делаем переливчатый стеклянный фон вместо сплошного
            let alpha = match glass { 1 => 40, 2 => 70, _ => 100 };
            let w = framebuffer::width();
            let h = framebuffer::height();
            // Frost overlay на весь экран (без blur для скорости)
            for y in 0..h {
                for x in 0..w {
                    if let Some(base) = framebuffer::get(x, y) {
                        framebuffer::put(x, y, framebuffer::alpha_blend(base, framebuffer::Rgb(255, 255, 255), alpha as u8));
                    }
                }
            }
        } else {
            framebuffer::clear(BG_COLOR);
        }
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
    write(b"  +-------------------------------------+\n");
    write(b"  |       PUREOS CRYSTAL KERNEL         |\n");
    write(b"  |    Immutable Ephemeral Kernel       |\n");
    write(b"  +-------------------------------------+\n");
    set_colors(FG, BG);
    write(b"\n");

    // System info
    let total_ram = crate::frame::total_physical_memory();
    let pool = crate::frame::stats();
    let cpu_count = crate::smp::cpu_count();

    set_colors(ACCENT, BG);
    write(b"  system");
    set_colors(FG, BG);
    write(b"\n");
    write(b"  cpu        "); write_num(cpu_count as u64); write(b" cores\n");
    write(b"  memory     "); write_num(total_ram / (1024 * 1024)); write(b" MiB total, ");
    write_num(pool.total_bytes / (1024 * 1024)); write(b" MiB pool\n");
    write(b"  display    "); write_num(crate::framebuffer::width() as u64);
    write(b"x"); write_num(crate::framebuffer::height() as u64); write(b" @32bpp\n");

    set_colors(ACCENT, BG);
    write(b"\n  commands");
    set_colors(FG, BG);
    write(b"\n");
    write(b"  help   - command reference\n");
    write(b"  info   - system information\n");
    write(b"  man    - documentation\n");
    write(b"  glass  - glassmorphism effect\n");
    write(b"  barrel - REPL interpreter\n");
    write(b"  top    - system monitor\n");
    write(b"\n");

    set_colors(ACCENT, BG);
    write(b"  v0.4  -  ready\n");
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

/// Получить текущий фоновый цвет терминала.
pub fn current_bg() -> Rgb {
    unsafe { BG_COLOR }
}

/// Получить текущий цвет текста терминала.
pub fn current_fg() -> Rgb {
    unsafe { FG_COLOR }
}

/// Включить/выключить glassmorphism-режим терминала.
/// level: 0=выкл, 1=лёгкий, 2=средний, 3=сильный
pub fn set_glass_mode(level: u32) {
    unsafe { GLASS_MODE = level.min(3); }
}

/// Получить текущий уровень glassmorphism.
pub fn glass_mode() -> u32 {
    unsafe { GLASS_MODE }
}

/// Установить цвета терминала с проверкой на glass-режим.
pub fn set_terminal_alpha(level: u32) {
    set_glass_mode(level);
    // Если стекло вкл — делаем фон более прозрачным на вид
    if level > 0 {
        apply_colors_from_config();
    }
}

/// Применить цвета из глобальной конфигурации.
pub fn apply_colors_from_config() {
    unsafe {
        let cfg = crate::config::get_config();
        FG_COLOR = Rgb(cfg.terminal_colors.foreground_r,
                       cfg.terminal_colors.foreground_g,
                       cfg.terminal_colors.foreground_b);
        BG_COLOR = Rgb(cfg.terminal_colors.background_r,
                       cfg.terminal_colors.background_g,
                       cfg.terminal_colors.background_b);
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

    let glass = GLASS_MODE;
    if glass > 0 {
        // Glassmorphism: blur + frost overlay вместо сплошного BG_COLOR
        let alpha = match glass {
            1 => 60,   // лёгкий — почти прозрачный
            2 => 100,  // средний — умеренный фрост
            _ => 140,  // сильный — плотный фрост
        };
        // Сначала рисуем обычный фон, сверху стекло
        framebuffer::draw_glass_background(x, y, CELL, CELL, alpha as u8);
    } else {
        // Обычный сплошной фон
        framebuffer::fill_rect(x, y, CELL, CELL, BG_COLOR);
    }

    // Глиф поверх фона
    let fw = font::font_width(SELECTED_FONT);
    let fh = font::font_height(SELECTED_FONT);
    for row in 0..fh as usize {
        for col in 0..fw {
            if font::glyph_pixel(SELECTED_FONT, ch, row, col) {
                framebuffer::fill_rect(
                    x + col * SCALE,
                    y + row as u32 * SCALE,
                    SCALE, SCALE, FG_COLOR);
            }
        }
    }
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
    if GLASS_MODE > 0 {
        // В glass-режиме — не сплошной фон, а легкий blur последней видимой строки
        let glass_alpha = match GLASS_MODE {
            1 => 60, 2 => 100, _ => 140,
        };
        framebuffer::draw_glass_background(0, h - CELL, w, CELL, glass_alpha as u8);
    } else {
        framebuffer::fill_rect(0, h - CELL, w, CELL, BG_COLOR);
    }
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

/// Установить позицию курсора (row, col). Без прокрутки.
pub fn set_cursor(row: u32, col: u32) {
    unsafe {
        CUR_ROW = row;
        CUR_COL = col;
    }
}


