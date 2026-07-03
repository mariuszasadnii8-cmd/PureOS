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

#[derive(Copy, Clone)]
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

/// Нарисовать символ с масштабом `scale` и выбранным шрифтом.
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

/// Нарисовать символ с масштабом `scale` (шрифт Compact по умолчанию).
pub fn draw_char_boot(x: u32, y: u32, ch: u8, c: Rgb, scale: u32) {
    draw_char(x, y, ch, c, scale, FontId::Compact);
}

/// Нарисовать строку.
pub fn draw_str(x: u32, y: u32, s: &[u8], c: Rgb, scale: u32) -> u32 {
    let mut cx = x;
    let advance = 8 * scale;
    for &ch in s {
        draw_char_boot(cx, y, ch, c, scale);
        cx += advance;
    }
    cx
}
