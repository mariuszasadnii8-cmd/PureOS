//! Framebuffer (GOP) — только для статического boot-экрана.
//!
//! Никакой анимации, никакого рендеринга терминала.
//! Просто запись пикселей в линейный фреймбуфер.
//! Zero-Alloc: всё состояние статично.

use core::ptr::write_volatile;

use crate::font::{self, FontId};

const FMT_RGB: u32 = 1;

static mut FB_BASE: u64 = 0;
static mut FB_W: u32 = 0;
static mut FB_H: u32 = 0;
static mut FB_STRIDE: u32 = 0;
static mut FB_FMT: u32 = 0;

#[derive(Copy, Clone, PartialEq)]
pub struct Rgb(pub u8, pub u8, pub u8);

pub unsafe fn init(base: u64, width: u32, height: u32, stride: u32, fmt: u32) {
    FB_BASE = base;
    FB_W = width;
    FB_H = height;
    FB_STRIDE = stride;
    FB_FMT = fmt;
}

#[inline(always)]
pub fn width() -> u32 { unsafe { FB_W } }

#[inline(always)]
pub fn height() -> u32 { unsafe { FB_H } }

#[inline(always)]
pub fn stride() -> u32 { unsafe { FB_STRIDE } }

#[inline(always)]
pub fn base() -> u64 { unsafe { FB_BASE } }

#[inline(always)]
fn enabled() -> bool { unsafe { FB_BASE != 0 && FB_W != 0 && FB_H != 0 } }

#[inline(always)]
fn pack(c: Rgb) -> u32 {
    let Rgb(r, g, b) = c;
    if unsafe { FB_FMT } == FMT_RGB {
        (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
    } else {
        (b as u32) | ((g as u32) << 8) | ((r as u32) << 16)
    }
}

#[inline(always)]
pub fn get(x: u32, y: u32) -> Option<Rgb> {
    if !enabled() { return None; }
    let (w, h, stride, base) = unsafe { (FB_W, FB_H, FB_STRIDE, FB_BASE) };
    if x >= w || y >= h { return None; }
    let offset = (y as u64 * stride as u64 + x as u64) * 4;
    let px = unsafe { core::ptr::read_volatile((base + offset) as *const u32) };
    Some(if unsafe { FB_FMT } == FMT_RGB {
        Rgb((px & 0xFF) as u8, ((px >> 8) & 0xFF) as u8, ((px >> 16) & 0xFF) as u8)
    } else {
        Rgb(((px >> 16) & 0xFF) as u8, ((px >> 8) & 0xFF) as u8, (px & 0xFF) as u8)
    })
}

#[inline(always)]
pub fn put(x: u32, y: u32, c: Rgb) {
    if !enabled() { return; }
    let (w, h, stride, base) = unsafe { (FB_W, FB_H, FB_STRIDE, FB_BASE) };
    if x >= w || y >= h { return; }
    let offset = (y as u64 * stride as u64 + x as u64) * 4;
    unsafe { write_volatile((base + offset) as *mut u32, pack(c)); }
}

pub fn fill_rect(x: u32, y: u32, w: u32, h: u32, c: Rgb) {
    for dy in 0..h {
        for dx in 0..w {
            put(x + dx, y + dy, c);
        }
    }
}

pub fn clear(c: Rgb) {
    fill_rect(0, 0, width(), height(), c);
}

/// Декодировать один UTF-8 символ из строки, вернуть codepoint.
/// Возвращает (codepoint, bytes_consumed).
fn utf8_decode(s: &[u8], pos: usize) -> (u16, usize) {
    if pos >= s.len() { return (0, 1); }
    let b0 = s[pos];
    if b0 < 0x80 {
        return (b0 as u16, 1);
    }
    if b0 >= 0xC0 && b0 <= 0xDF && pos + 1 < s.len() {
        let b1 = s[pos + 1];
        let cp = ((b0 as u16 & 0x1F) << 6) | (b1 as u16 & 0x3F);
        return (cp, 2);
    }
    if b0 >= 0xE0 && b0 <= 0xEF && pos + 2 < s.len() {
        let b1 = s[pos + 1];
        let b2 = s[pos + 2];
        let cp = ((b0 as u16 & 0x0F) << 12) | ((b1 as u16 & 0x3F) << 6) | (b2 as u16 & 0x3F);
        return (cp, 3);
    }
    // Fallback: treat as raw byte
    (b0 as u16, 1)
}

/// Нарисовать символ с масштабом `scale` и выбранным шрифтом.
/// Поддерживает UTF-8 (Cyrillic).
pub fn draw_char(x: u32, y: u32, ch: u8, c: Rgb, scale: u32, font_id: FontId) {
    let fw = font::font_width(font_id);
    let fh = font::font_height(font_id);
    for row in 0..fh as usize {
        for col in 0..fw {
            if font::glyph_pixel(font_id, ch, row, col) {
                fill_rect(x + col * scale, y + row as u32 * scale, scale, scale, c);
            }
        }
    }
}

/// Нарисовать символ по codepoint (поддерживает Cyrillic).
pub fn draw_char_cp(x: u32, y: u32, cp: u16, c: Rgb, scale: u32, font_id: FontId) {
    let fw = font::font_width(font_id);
    let fh = font::font_height(font_id);
    for row in 0..fh as usize {
        for col in 0..fw {
            if font::glyph_pixel_codepoint(font_id, cp, row, col) {
                fill_rect(x + col * scale, y + row as u32 * scale, scale, scale, c);
            }
        }
    }
}

/// Нарисовать символ с масштабом `scale` (шрифт Compact по умолчанию).
pub fn draw_char_boot(x: u32, y: u32, ch: u8, c: Rgb, scale: u32) {
    draw_char(x, y, ch, c, scale, FontId::Compact);
}

/// Нарисовать строку (ASCII).
pub fn draw_str(x: u32, y: u32, s: &[u8], c: Rgb, scale: u32) -> u32 {
    let mut cx = x;
    let advance = 8 * scale;
    for &ch in s {
        draw_char_boot(cx, y, ch, c, scale);
        cx += advance;
    }
    cx
}

/// Нарисовать UTF-8 строку (поддерживает Cyrillic и ASCII).
pub fn draw_str_utf8(x: u32, y: u32, s: &[u8], c: Rgb, scale: u32) -> u32 {
    let mut cx = x;
    let advance = 8 * scale;
    let mut pos = 0;
    while pos < s.len() {
        let (cp, consumed) = utf8_decode(s, pos);
        if cp != 0 {
            if cp < 128 {
                draw_char_boot(cx, y, cp as u8, c, scale);
            } else {
                draw_char_cp(cx, y, cp, c, scale, FontId::Compact);
            }
            cx += advance;
        }
        pos += consumed;
    }
    cx
}

// ═══════════════════════════════════════════════════════════════════
// Glassmorphism — вспомогательные функции
// ═══════════════════════════════════════════════════════════════════

/// Альфа-смешивание: base цвет с overlay цветом (0..255 alpha).
#[inline]
pub fn alpha_blend(base: Rgb, overlay: Rgb, alpha: u8) -> Rgb {
    let a = alpha as u32;
    let inv_a = 255u32.wrapping_sub(a);
    Rgb(
        ((base.0 as u32 * inv_a + overlay.0 as u32 * a) / 255) as u8,
        ((base.1 as u32 * inv_a + overlay.1 as u32 * a) / 255) as u8,
        ((base.2 as u32 * inv_a + overlay.2 as u32 * a) / 255) as u8,
    )
}

/// Быстрый box blur 1 проход (радиус 1) для прямоугольной области.
/// Использует стриминг без выделения большого буфера.
pub unsafe fn box_blur(x: u32, y: u32, w: u32, h: u32) {
    if w < 3 || h < 3 { return; }
    // Обрабатываем без выделения буфера: читаем 3 строки за раз
    let mut row0: [Rgb; 256] = [Rgb(0, 0, 0); 256];
    let mut row1: [Rgb; 256] = [Rgb(0, 0, 0); 256];
    let mut row2: [Rgb; 256] = [Rgb(0, 0, 0); 256];
    let cw = w.min(256) as usize;
    if cw < 3 { return; }
    // Строка 0 и 1
    for dx in 0..cw {
        row0[dx] = get(x + dx as u32, y).unwrap_or(Rgb(0,0,0));
        row1[dx] = get(x + dx as u32, y + 1).unwrap_or(Rgb(0,0,0));
    }
    for dy in 1..h-1 {
        // Строка dy+1 (вперёд)
        for dx in 0..cw {
            row2[dx] = get(x + dx as u32, y + dy + 1).unwrap_or(Rgb(0,0,0));
        }
        // Blur строки dy с row0, row1, row2
        for dx in 1..cw-1 {
            let mut r: u32 = 0;
            let mut g: u32 = 0;
            let mut b: u32 = 0;
            for ky in 0..3 {
                let row = if ky == 0 { &row0 } else if ky == 1 { &row1 } else { &row2 };
                for kx in 0..3 {
                    let p = row[dx + kx - 1];
                    r += p.0 as u32;
                    g += p.1 as u32;
                    b += p.2 as u32;
                }
            }
            put(x + dx as u32, y + dy, Rgb((r / 9) as u8, (g / 9) as u8, (b / 9) as u8));
        }
        // Сдвиг строк
        core::mem::swap(&mut row0, &mut row1);
        core::mem::swap(&mut row1, &mut row2);
    }
}

/// Нарисовать стеклянную (glassmorphism) подложку на область:
///  1. blur фона (если область < 200px)
///  2. overlay белым с alpha-прозрачностью
///  3. тонкая полупрозрачная рамка
pub unsafe fn draw_glass_background(x: u32, y: u32, w: u32, h: u32, frost_alpha: u8) {
    if w == 0 || h == 0 { return; }
    // 1. Blur (только для небольших областей вроде ячеек терминала, не для полос прокрутки)
    if w <= 64 && h <= 64 {
        box_blur(x, y, w, h);
    }
    // 2. Frost overlay (всегда)
    if h <= 128 && w <= 512 {
        for dy in 0..h {
            for dx in 0..w {
                if let Some(base) = get(x + dx, y + dy) {
                    put(x + dx, y + dy, alpha_blend(base, Rgb(255, 255, 255), frost_alpha));
                }
            }
        }
    } else {
        // Для больших областей — быстрая заливка
        fill_rect(x, y, w, h, alpha_blend(Rgb(30, 30, 40), Rgb(255, 255, 255), frost_alpha));
    }
    // 3. Рамка (только если маленькая область)
    if w <= 128 && h <= 128 {
        let border = alpha_blend(Rgb(0, 0, 0), Rgb(255, 255, 255), 80);
        for dx in 0..w {
            put(x + dx, y, border);
            put(x + dx, y + h - 1, border);
        }
        for dy in 0..h {
            put(x, y + dy, border);
            put(x + w - 1, y + dy, border);
        }
    }
}
