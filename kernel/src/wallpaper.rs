//! Procedural wallpaper generators + custom loading.
//! Zero-Alloc: renders directly to framebuffer. No f32 math (no_std sin/cos).

use crate::framebuffer::{self, Rgb};

#[derive(Clone, Copy, PartialEq)]
pub enum WallpaperId {
    Solid,
    Gradient,
    Stripes,
    Checkers,
    Radial,
    Waves,
    Grid,
    Noise,
    Custom1, // bg.bin — 1672×941 32bpp BGRA
}

pub const WALLPAPER_COUNT: usize = 9;

/// Встроенные данные обоев (raw 32bpp BGRA).
static WALLPAPER1_DATA: &[u8] = include_bytes!("../../userspace/personal/bg.bin");

const WALLPAPER1_W: u32 = 1672;
const WALLPAPER1_H: u32 = 941;

pub fn wallpaper_name(id: WallpaperId) -> &'static [u8] {
    match id {
        WallpaperId::Solid => b"solid",
        WallpaperId::Gradient => b"gradient",
        WallpaperId::Stripes => b"stripes",
        WallpaperId::Checkers => b"checkers",
        WallpaperId::Radial => b"radial",
        WallpaperId::Waves => b"waves",
        WallpaperId::Grid => b"grid",
        WallpaperId::Noise => b"noise",
        WallpaperId::Custom1 => b"bg1",
    }
}

fn lookup(name: &[u8]) -> Option<WallpaperId> {
    let ids = [
        WallpaperId::Solid, WallpaperId::Gradient, WallpaperId::Stripes,
        WallpaperId::Checkers, WallpaperId::Radial, WallpaperId::Waves,
        WallpaperId::Grid, WallpaperId::Noise, WallpaperId::Custom1,
    ];
    for id in &ids {
        if wallpaper_name(*id) == name {
            return Some(*id);
        }
    }
    None
}

/// Нарисовать raw 32bpp BGRA изображение на весь фреймбуфер с nearest-neighbor scaling.
fn draw_raw_bgra(data: &[u8], img_w: u32, img_h: u32) {
    let fb_w = framebuffer::width();
    let fb_h = framebuffer::height();
    if fb_w == 0 || fb_h == 0 || data.len() < (img_w * img_h * 4) as usize { return; }
    for y in 0..fb_h {
        let src_y = (y * img_h) / fb_h;
        let row_off = (src_y * img_w * 4) as usize;
        for x in 0..fb_w {
            let src_x = (x * img_w) / fb_w;
            let off = row_off + (src_x * 4) as usize;
            if off + 3 >= data.len() { continue; }
            let b = data[off];
            let g = data[off + 1];
            let r = data[off + 2];
            framebuffer::put(x, y, Rgb(r, g, b));
        }
    }
}

/// Approximate sine using a lookup-like polynomial for u32 in 0..255 range (0..2π).
fn sin256(x: u32) -> i32 {
    // Simple triangle wave as sin approximation
    let a = x % 256;
    if a < 64 { (a as i32) * 4 }           // 0 → 0..255
    else if a < 128 { (128 - a as i32) * 4 } // 255..0
    else if a < 192 { ((a - 128) as i32) * -4 } // 0..-255
    else { (256 - a as i32) * 4 }          // -255..0
}

pub fn draw(wallpaper: WallpaperId) {
    let w = framebuffer::width();
    let h = framebuffer::height();
    if w == 0 || h == 0 { return; }

    match wallpaper {
        WallpaperId::Solid => {
            framebuffer::clear(Rgb(10, 15, 25));
        }
        WallpaperId::Gradient => {
            for y in 0..h {
                let t = (y * 255) / h;
                let r = (10 + (t * 50) / 255) as u8;
                let g = (15 + (t * 40) / 255) as u8;
                let b = (25 + (t * 80) / 255) as u8;
                for x in 0..w {
                    framebuffer::put(x, y, Rgb(r, g, b));
                }
            }
        }
        WallpaperId::Stripes => {
            for y in 0..h {
                let c = if (y / 16) % 2 == 0 { Rgb(20, 30, 50) } else { Rgb(10, 15, 25) };
                for x in 0..w {
                    framebuffer::put(x, y, c);
                }
            }
        }
        WallpaperId::Checkers => {
            for y in 0..h {
                for x in 0..w {
                    let c = if ((x / 32) + (y / 32)) % 2 == 0 {
                        Rgb(20, 30, 50)
                    } else {
                        Rgb(40, 55, 80)
                    };
                    framebuffer::put(x, y, c);
                }
            }
        }
        WallpaperId::Radial => {
            let cx = w as i32 / 2;
            let cy = h as i32 / 2;
            let max_d = ((cx * cx + cy * cy) as u32).isqrt();
            if max_d == 0 { return; }
            for y in 0..h {
                let dy = y as i32 - cy;
                for x in 0..w {
                    let dx = x as i32 - cx;
                    let d = ((dx * dx + dy * dy) as u32).isqrt() * 255 / max_d;
                    let t = 255u32.saturating_sub(d);
                    let r = (10 + (t * 60) / 255) as u8;
                    let g = (15 + (t * 50) / 255) as u8;
                    let b = (25 + (t * 100) / 255) as u8;
                    framebuffer::put(x, y, Rgb(r, g, b));
                }
            }
        }
        WallpaperId::Waves => {
            for y in 0..h {
                for x in 0..w {
                    let sx = (x * 13) % 256;
                    let sy = (y * 13) % 256;
                    let wave = (sin256(sx).unsigned_abs() * sin256((sy + 64) % 256).unsigned_abs()) / 65536;
                    let r = (10 + (wave * 80) / 255) as u8;
                    let g = (15 + (wave * 60) / 255) as u8;
                    let b = (25 + (wave * 120) / 255) as u8;
                    framebuffer::put(x, y, Rgb(r, g, b));
                }
            }
        }
        WallpaperId::Grid => {
            framebuffer::clear(Rgb(10, 15, 25));
            let step = 64u32;
            for x in (0..w).step_by(step as usize) {
                for dy in 0..h {
                    framebuffer::put(x, dy, Rgb(40, 55, 80));
                }
                if x + 1 < w {
                    for dy in 0..h {
                        framebuffer::put(x + 1, dy, Rgb(30, 40, 60));
                    }
                }
            }
            for y in (0..h).step_by(step as usize) {
                for dx in 0..w {
                    framebuffer::put(dx, y, Rgb(40, 55, 80));
                }
                if y + 1 < h {
                    for dx in 0..w {
                        framebuffer::put(dx, y + 1, Rgb(30, 40, 60));
                    }
                }
            }
        }
        WallpaperId::Noise => {
            for y in 0..h {
                for x in 0..w {
                    let n = ((x * 17 + y * 31) & 0xFF) as u8;
                    framebuffer::put(x, y, Rgb(n / 3, n / 4, n / 2));
                }
            }
        }
        WallpaperId::Custom1 => {
            draw_raw_bgra(WALLPAPER1_DATA, WALLPAPER1_W, WALLPAPER1_H);
        }
    }
}

pub fn set_wallpaper_by_name(name: &[u8]) -> bool {
    if let Some(id) = lookup(name) {
        draw(id);
        true
    } else {
        false
    }
}
