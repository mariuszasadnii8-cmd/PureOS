//! Игры, демки и приколы для PureOS.
//!
//! Содержит: snake (игра), matrix (цифровой дождь),
//! mandelbrot (фрактал), starfield (3D звёзды),
//! fire (огонь). Все работают напрямую с фреймбуфером.

use crate::framebuffer;
use crate::framebuffer::Rgb;
use crate::keyboard;
use crate::cpu;
use crate::font::FontId;

// ═══════════════════════════════════════════════════════════════════
// Примитивы
// ═══════════════════════════════════════════════════════════════════

fn rand(seed: &mut u32) -> u32 {
    *seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
    (*seed >> 16) & 0x7FFF
}

unsafe fn delay_us(us: u32) {
    for _ in 0..us * 4 {
        cpu::inb(0x80);
    }
}

unsafe fn delay_ms(ms: u32) {
    delay_us(ms * 1000);
}

fn try_key() -> Option<u8> {
    unsafe { keyboard::read_key() }
}

unsafe fn wait_key() -> u8 {
    loop {
        if let Some(k) = try_key() {
            return k;
        }
        delay_ms(5);
    }
}

// ═══════════════════════════════════════════════════════════════════
// SNAKE
// ═══════════════════════════════════════════════════════════════════

const SNAKE_MAX: usize = 256;

struct SnakeGame {
    seg_x: [i32; SNAKE_MAX],
    seg_y: [i32; SNAKE_MAX],
    len: usize,
    dir: u8,
    next_dir: u8,
    food_x: i32,
    food_y: i32,
    cols: i32,
    rows: i32,
    score: u32,
    seed: u32,
    game_over: bool,
}

/// Сыграть в змейку. ←↑↓→ или WASD. ESC — выход.
pub unsafe fn play_snake() {
    let fw = framebuffer::width();
    let fh = framebuffer::height();

    let cell = 12i32;
    let cols = (fw as i32) / cell;
    let rows = (fh as i32) / cell;
    let ox = (fw as i32 - cols * cell) / 2;
    let oy = (fh as i32 - rows * cell) / 2;

    let mut seed: u32 = cpu::inb(0x40) as u32
        | (cpu::inb(0x40) as u32) << 8
        | (cpu::inb(0x40) as u32) << 16;

    let mut snake = SnakeGame {
        seg_x: [0; SNAKE_MAX],
        seg_y: [0; SNAKE_MAX],
        len: 3,
        dir: 3,
        next_dir: 3,
        food_x: 0,
        food_y: 0,
        cols,
        rows,
        score: 0,
        seed,
        game_over: false,
    };

    let start_x = cols / 2;
    let start_y = rows / 2;
    for i in 0..snake.len {
        snake.seg_x[i] = start_x - i as i32;
        snake.seg_y[i] = start_y;
    }

    snake.spawn_food();

    let mut frame = 0u32;
    let bg = Rgb(10, 10, 15);

    loop {
        // Input
        while let Some(k) = try_key() {
            match k {
                0x1B => return,
                b'w' | b'W' => { if snake.dir != 1 { snake.next_dir = 0; } }
                b's' | b'S' => { if snake.dir != 0 { snake.next_dir = 1; } }
                b'a' | b'A' => { if snake.dir != 3 { snake.next_dir = 2; } }
                b'd' | b'D' => { if snake.dir != 2 { snake.next_dir = 3; } }
                _ => {}
            }
            // Arrows via escape sequences
            if k == 0x1B {
                let nxt1 = if let Some(n) = try_key() { n } else { continue };
                if nxt1 == b'[' {
                    let nxt2 = if let Some(n) = try_key() { n } else { continue };
                    match nxt2 {
                        b'A' => { if snake.dir != 1 { snake.next_dir = 0; } }
                        b'B' => { if snake.dir != 0 { snake.next_dir = 1; } }
                        b'D' => { if snake.dir != 3 { snake.next_dir = 2; } }
                        b'C' => { if snake.dir != 2 { snake.next_dir = 3; } }
                        _ => {}
                    }
                }
            }
        }

        if snake.game_over {
            let mx = (fw as i32 / 2 - 140).max(0) as u32;
            framebuffer::fill_rect(mx, fh / 2 - 30, 320, 64, Rgb(20, 10, 10));
            framebuffer::draw_str(mx + 8, fh / 2 - 20, b"GAME OVER", Rgb(255, 80, 80), 2);
            framebuffer::draw_str(mx + 8, fh / 2, b"Score:     ", Rgb(200, 200, 100), 1);
            // Print score number
            let mut sn = snake.score;
            let mut si = 0u32;
            let mut sdigits: [u8; 10] = [0; 10];
            if sn == 0 { sdigits[0] = b'0'; si = 1; }
            else { while sn > 0 { sdigits[si as usize] = (sn % 10) as u8 + b'0'; sn /= 10; si += 1; } }
            for j in 0..si {
                framebuffer::draw_char(mx + 56 + j as u32 * 8, fh / 2, sdigits[si as usize - 1 - j as usize], Rgb(255, 200, 100), 1, FontId::Vga);
            }

            framebuffer::draw_str(mx + 8, fh / 2 + 16, b"ENTER=restart  ESC=quit", Rgb(120, 120, 120), 1);

            let k = wait_key();
            if k == 0x1B { return; }
            if k == b'\r' || k == b'\n' {
                framebuffer::clear(bg);
                snake.len = 3;
                snake.dir = 3;
                snake.next_dir = 3;
                snake.score = 0;
                snake.game_over = false;
                for i in 0..snake.len {
                    snake.seg_x[i] = start_x - i as i32;
                    snake.seg_y[i] = start_y;
                }
                snake.spawn_food();
                frame = 0;
                continue;
            }
            continue;
        }

        frame += 1;
        if frame % 6 == 0 {
            snake.dir = snake.next_dir;

            let tail_x = snake.seg_x[snake.len - 1];
            let tail_y = snake.seg_y[snake.len - 1];

            for i in (1..snake.len).rev() {
                snake.seg_x[i] = snake.seg_x[i - 1];
                snake.seg_y[i] = snake.seg_y[i - 1];
            }

            match snake.dir {
                0 => snake.seg_y[0] -= 1,
                1 => snake.seg_y[0] += 1,
                2 => snake.seg_x[0] -= 1,
                3 => snake.seg_x[0] += 1,
                _ => {}
            }

            if snake.seg_x[0] < 0 || snake.seg_x[0] >= snake.cols
                || snake.seg_y[0] < 0 || snake.seg_y[0] >= snake.rows
            {
                snake.game_over = true;
                continue;
            }

            for i in 1..snake.len {
                if snake.seg_x[i] == snake.seg_x[0] && snake.seg_y[i] == snake.seg_y[0] {
                    snake.game_over = true;
                    break;
                }
            }
            if snake.game_over { continue; }

            if snake.seg_x[0] == snake.food_x && snake.seg_y[0] == snake.food_y {
                snake.score += 10;
                if snake.len < SNAKE_MAX {
                    snake.seg_x[snake.len] = tail_x;
                    snake.seg_y[snake.len] = tail_y;
                    snake.len += 1;
                }
                snake.spawn_food();
            }
        }

        // Render
        framebuffer::clear(bg);

        for r in 0..snake.rows {
            for c in 0..snake.cols {
                let fx = (ox + c * cell) as u32;
                let fy = (oy + r * cell) as u32;
                if (c + r) & 1 == 0 {
                    framebuffer::put(fx, fy, Rgb(14, 16, 22));
                } else {
                    framebuffer::put(fx, fy, Rgb(12, 14, 18));
                }
            }
        }

        for i in 0..snake.len {
            let sx = snake.seg_x[i];
            let sy = snake.seg_y[i];
            if sx >= 0 && sx < snake.cols && sy >= 0 && sy < snake.rows {
                let px = (ox + sx * cell) as u32;
                let py = (oy + sy * cell) as u32;
                let color = if i == 0 { Rgb(80, 220, 120) } else {
                    let t = 180 - (i as u8 * 5).min(120);
                    Rgb(40, t, 60)
                };
                framebuffer::fill_rect(px + 1, py + 1, (cell - 2) as u32, (cell - 2) as u32, color);
            }
        }

        let fx = (ox + snake.food_x * cell) as u32;
        let fy = (oy + snake.food_y * cell) as u32;
        framebuffer::fill_rect(fx + 2, fy + 2, (cell - 4) as u32, (cell - 4) as u32, Rgb(220, 60, 60));
        framebuffer::fill_rect(fx + 4, fy + 4, (cell - 8) as u32, (cell - 8) as u32, Rgb(255, 100, 80));

        framebuffer::draw_str(
            (fw - 90) as u32, 4, b"ESC=quit", Rgb(100, 100, 100), 1,
        );
    }
}

impl SnakeGame {
    fn spawn_food(&mut self) {
        loop {
            self.food_x = (rand(&mut self.seed) as i32) % self.cols;
            self.food_y = (rand(&mut self.seed) as i32) % self.rows;
            let mut on_snake = false;
            for i in 0..self.len {
                if self.seg_x[i] == self.food_x && self.seg_y[i] == self.food_y {
                    on_snake = true;
                    break;
                }
            }
            if !on_snake { break; }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// MATRIX RAIN
// ═══════════════════════════════════════════════════════════════════

pub unsafe fn matrix_rain() {
    let fw = framebuffer::width();
    let fh = framebuffer::height();
    let cols = (fw / 10) as usize;
    let rows = (fh / 12) as usize;

    let mut seed: u32 = cpu::inb(0x40) as u32 | (cpu::inb(0x40) as u32) << 8;

    let max_cols = cols.min(200);
    let mut drops: [i32; 200] = [0; 200];
    let mut speeds: [i32; 200] = [1; 200];
    let mut lengths: [i32; 200] = [10; 200];
    let mut cbuf: [u8; 200] = [0; 200];

    for c in 0..max_cols {
        drops[c] = (rand(&mut seed) as i32) % rows as i32;
        speeds[c] = 1 + (rand(&mut seed) % 3) as i32;
        lengths[c] = 3 + (rand(&mut seed) % 12) as i32;
        cbuf[c] = m_char(&mut seed);
    }

    for _ in 0..400 {
        while let Some(k) = try_key() {
            if k == 0x1B {
                framebuffer::clear(Rgb(0, 0, 0));
                return;
            }
        }

        // Fade all
        for y in 0..fh {
            for x in 0..fw {
                let p = framebuffer::get(x, y).unwrap_or(Rgb(0, 0, 0));
                let g = p.1.saturating_sub(6);
                framebuffer::put(x, y, Rgb(0, g, 0));
            }
        }

        for c in 0..max_cols {
            let xx = c as u32 * 10;
            let len = lengths[c];

            for r in 0..len {
                let yy = (drops[c] - r) as i32;
                if yy < 0 || yy >= rows as i32 { continue; }
                let py = yy as u32 * 12;
                let dist = r as f32 / len as f32;
                let bright = if r == 0 {
                    220
                } else {
                    (120.0 * (1.0 - dist)) as u8
                };
                let ch = if r == 0 { m_char(&mut seed) } else { cbuf[c] };
                framebuffer::draw_char(xx, py, ch, Rgb(0, bright, 0), 1, FontId::Vga);
            }

            drops[c] += speeds[c];
            if drops[c] >= rows as i32 + lengths[c] {
                drops[c] = -(rand(&mut seed) as i32 % 10);
                lengths[c] = 3 + (rand(&mut seed) % 12) as i32;
                cbuf[c] = m_char(&mut seed);
            }
        }

        delay_ms(60);
    }
    framebuffer::clear(Rgb(0, 0, 0));
}

fn m_char(seed: &mut u32) -> u8 {
    let r = rand(seed) % 60;
    if r < 20 { b'A' + (rand(seed) % 26) as u8 }
    else if r < 40 { b'0' + (rand(seed) % 10) as u8 }
    else {
        let syms = b":;.,!?+-*/=$%&@[]<>~#";
        syms[(rand(seed) as usize) % syms.len()]
    }
}

// ═══════════════════════════════════════════════════════════════════
// MANDELBROT
// ═══════════════════════════════════════════════════════════════════

pub unsafe fn mandelbrot() {
    let fw = framebuffer::width();
    let fh = framebuffer::height();

    for py in 0..fw.min(fh).min(512) {
        for px in 0..fw.min(fh).min(512) {
            let cx = -2.2 + (3.0 * px as f32) / (fw.min(fh).min(512) as f32);
            let cy = -1.2 + (2.4 * py as f32) / (fw.min(fh).min(512) as f32);

            let mut zx = 0.0f32;
            let mut zy = 0.0f32;
            let mut i = 0u32;
            while i < 64 {
                let zx2 = zx * zx;
                let zy2 = zy * zy;
                if zx2 + zy2 > 4.0 { break; }
                let xt = zx2 - zy2 + cx;
                zy = 2.0 * zx * zy + cy;
                zx = xt;
                i += 1;
            }

            let color = if i >= 64 { Rgb(0, 0, 0) } else {
                let t = i as f32 / 64.0;
                Rgb((t * 255.0) as u8, ((t * 200.0 + 55.0) as u8), (255.0 * (1.0 - t)) as u8)
            };
            framebuffer::put(px, py, color);
        }
    }

    framebuffer::draw_str(fw / 2 - 60, fh - 16, b"PRESS ESC TO EXIT", Rgb(120, 120, 120), 1);
    loop {
        if let Some(k) = try_key() {
            if k == 0x1B { break; }
        }
        delay_ms(20);
    }
    framebuffer::clear(Rgb(0, 0, 0));
}

// ═══════════════════════════════════════════════════════════════════
// STARFIELD
// ═══════════════════════════════════════════════════════════════════

pub unsafe fn starfield() {
    let fw = framebuffer::width();
    let fh = framebuffer::height();
    let cx = fw as f32 / 2.0;
    let cy = fh as f32 / 2.0;

    let mut seed: u32 = cpu::inb(0x40) as u32 | (cpu::inb(0x40) as u32) << 8;
    let mut stars = [Star { x: 0.0, y: 0.0, z: 0.0 }; 128];

    for s in stars.iter_mut() {
        s.x = (rand(&mut seed) as f32 / 32767.0) * 2000.0 - 1000.0;
        s.y = (rand(&mut seed) as f32 / 32767.0) * 2000.0 - 1000.0;
        s.z = (rand(&mut seed) as f32 / 32767.0) * 500.0 + 1.0;
    }

    for _ in 0..500 {
        while let Some(k) = try_key() {
            if k == 0x1B {
                framebuffer::clear(Rgb(0, 0, 0));
                return;
            }
        }

        framebuffer::clear(Rgb(0, 0, 5));

        for s in stars.iter_mut() {
            s.z -= 5.0;
            if s.z <= 0.0 {
                s.x = (rand(&mut seed) as f32 / 32767.0) * 2000.0 - 1000.0;
                s.y = (rand(&mut seed) as f32 / 32767.0) * 2000.0 - 1000.0;
                s.z = 500.0;
            }

            let kk = 256.0 / s.z;
            let px = (s.x * kk + cx) as i32;
            let py = (s.y * kk + cy) as i32;

            if px >= 0 && px < fw as i32 && py >= 0 && py < fh as i32 {
                let bright = (255.0 * (1.0 - s.z / 500.0)) as u8;
                framebuffer::put(px as u32, py as u32, Rgb(bright, bright, bright));
            }
        }

        delay_ms(20);
    }
    framebuffer::clear(Rgb(0, 0, 0));
}

#[derive(Clone, Copy)]
struct Star {
    x: f32,
    y: f32,
    z: f32,
}

// ═══════════════════════════════════════════════════════════════════
// FIRE
// ═══════════════════════════════════════════════════════════════════

const FIRE_W: usize = 100;
const FIRE_H: usize = 64;
static mut FIRE_BUF: [u8; FIRE_W * FIRE_H] = [0; FIRE_W * FIRE_H];

pub unsafe fn fire() {
    let fw = framebuffer::width();
    let fh = framebuffer::height();

    let mut seed: u32 = cpu::inb(0x40) as u32 | (cpu::inb(0x40) as u32) << 8;

    // Init bottom row
    for x in 0..FIRE_W {
        FIRE_BUF[(FIRE_H - 1) * FIRE_W + x] = 255;
    }

    let cell_w = (fw as usize / FIRE_W).max(4);
    let cell_h = (fh as usize / FIRE_H).max(4);

    loop {
        while let Some(k) = try_key() {
            if k == 0x1B {
                framebuffer::clear(Rgb(0, 0, 0));
                return;
            }
        }

        // Simulate: propagate upward
        for y in 1..FIRE_H {
            for x in 0..FIRE_W {
                let src = y * FIRE_W + x;
                let below = (y - 1) * FIRE_W + x;
                let val = FIRE_BUF[below] as i32;
                let dec = (val - (rand(&mut seed) % 4) as i32 - 2).max(0) as u8;
                FIRE_BUF[src] = dec;
            }
        }

        // Sparks bottom
        for _ in 0..4 {
            let sx = rand(&mut seed) as usize % FIRE_W;
            FIRE_BUF[(FIRE_H - 1) * FIRE_W + sx] = 255;
        }

        // Render
        for y in 0..FIRE_H {
            for x in 0..FIRE_W {
                let val = FIRE_BUF[y * FIRE_W + x];
                let (r, g, b) = fire_pal(val);
                framebuffer::fill_rect(
                    (x * cell_w) as u32, (y * cell_h) as u32,
                    cell_w as u32, cell_h as u32, Rgb(r, g, b),
                );
            }
        }

        delay_ms(30);
    }
}

fn fire_pal(v: u8) -> (u8, u8, u8) {
    if v < 64 {
        (v * 2, 0, 0)
    } else if v < 128 {
        (255, (v - 64) * 4, 0)
    } else if v < 192 {
        (255, 255 - (v - 128) * 4, (v - 128) * 4)
    } else {
        (255, 255, 255)
    }
}

// ═══════════════════════════════════════════════════════════════════
// BOUNCE — DVD-style логотип
// ═══════════════════════════════════════════════════════════════════

pub unsafe fn bounce() {
    let fw = framebuffer::width();
    let fh = framebuffer::height();

    let mut x = 0i32;
    let mut y = 30i32;
    let mut dx = 3i32;
    let mut dy = 3i32;

    let colors = [
        Rgb(200, 100, 255), Rgb(100, 200, 255), Rgb(100, 255, 100),
        Rgb(255, 200, 100), Rgb(255, 100, 200), Rgb(200, 200, 100),
    ];
    let mut ci = 0usize;
    let logo = b"PUREOS";
    let lw = (logo.len() * 16) as i32;
    let lh = 16i32;

    loop {
        while let Some(k) = try_key() {
            if k == 0x1B {
                framebuffer::clear(Rgb(0, 0, 0));
                return;
            }
        }

        framebuffer::clear(Rgb(5, 0, 10));
        x += dx; y += dy;

        if x + lw >= fw as i32 { dx = -dx; x = fw as i32 - lw; ci = (ci + 1) % colors.len(); }
        if x <= 0 { dx = -dx; x = 0; ci = (ci + 1) % colors.len(); }
        if y + lh >= fh as i32 { dy = -dy; y = fh as i32 - lh; ci = (ci + 1) % colors.len(); }
        if y <= 0 { dy = -dy; y = 0; ci = (ci + 1) % colors.len(); }

        framebuffer::draw_str(x as u32, y as u32, logo, colors[ci], 2);
        delay_ms(30);
    }
}

// ═══════════════════════════════════════════════════════════════════
// Демо-раннер
// ═══════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════
// RAIN — дождь
// ═══════════════════════════════════════════════════════════════════

pub unsafe fn rain() {
    let fw = framebuffer::width();
    let fh = framebuffer::height();
    let mut seed: u32 = cpu::inb(0x40) as u32 | (cpu::inb(0x40) as u32) << 8;

    let n = 120usize;
    let mut drops_x: [i32; 120] = [0; 120];
    let mut drops_y: [i32; 120] = [0; 120];
    let mut drops_s: [i32; 120] = [0; 120]; // speed

    for i in 0..n {
        drops_x[i] = (rand(&mut seed) as i32) % fw as i32;
        drops_y[i] = (rand(&mut seed) as i32) % fh as i32;
        drops_s[i] = 2 + (rand(&mut seed) % 4) as i32;
    }

    for _ in 0..400 {
        while let Some(k) = try_key() {
            if k == 0x1B { framebuffer::clear(Rgb(0,0,0)); return; }
        }

        // Fade
        framebuffer::fill_rect(0, 0, fw, fh, Rgb(5, 5, 8));

        for i in 0..n {
            drops_y[i] += drops_s[i];
            if drops_y[i] >= fh as i32 + 10 {
                drops_y[i] = -10;
                drops_x[i] = (rand(&mut seed) as i32) % fw as i32;
            }

            let x = drops_x[i] as u32;
            let y = drops_y[i] as u32;
            if x < fw && y < fh {
                framebuffer::put(x, y, Rgb(100, 140, 180));
                if y >= 1 {
                    framebuffer::put(x, y - 1, Rgb(60, 100, 140));
                }
                if y >= 2 {
                    framebuffer::put(x, y - 2, Rgb(30, 60, 100));
                }
            }
        }

        delay_ms(30);
    }
    framebuffer::clear(Rgb(0,0,0));
}

// ═══════════════════════════════════════════════════════════════════
// SNOW — снегопад
// ═══════════════════════════════════════════════════════════════════

pub unsafe fn snow() {
    let fw = framebuffer::width();
    let fh = framebuffer::height();
    let mut seed: u32 = cpu::inb(0x40) as u32 | (cpu::inb(0x40) as u32) << 8;

    let n = 200usize;
    let mut sx: [i32; 200] = [0; 200];
    let mut sy: [i32; 200] = [0; 200];
    let mut ss: [i32; 200] = [0; 200]; // speed
    let mut sw: [u8; 200] = [0; 200];  // size

    for i in 0..n {
        sx[i] = (rand(&mut seed) as i32) % fw as i32;
        sy[i] = (rand(&mut seed) as i32) % fh as i32;
        ss[i] = 1 + (rand(&mut seed) % 3) as i32;
        sw[i] = 1 + (rand(&mut seed) % 2) as u8;
    }

    for _ in 0..500 {
        while let Some(k) = try_key() {
            if k == 0x1B { framebuffer::clear(Rgb(0,0,0)); return; }
        }

        framebuffer::fill_rect(0, 0, fw, fh, Rgb(5, 5, 15));

        for i in 0..n {
            sy[i] += ss[i];
            sx[i] += (rand(&mut seed) as i32 % 3) - 1; // random drift
            if sy[i] >= fh as i32 + 5 {
                sy[i] = -5;
                sx[i] = (rand(&mut seed) as i32) % fw as i32;
            }
            if sx[i] < 0 { sx[i] = fw as i32 - 1; }
            if sx[i] >= fw as i32 { sx[i] = 0; }

            let x = sx[i] as u32;
            let y = sy[i] as u32;
            let bright = 180 + (rand(&mut seed) % 60) as u8;
            if sw[i] == 1 {
                framebuffer::put(x, y, Rgb(bright, bright, bright));
            } else {
                framebuffer::fill_rect(x, y, 2, 2, Rgb(bright, bright, bright));
            }
        }

        delay_ms(40);
    }
    framebuffer::clear(Rgb(0,0,0));
}

// ═══════════════════════════════════════════════════════════════════
// KALEIDOSCOPE — цветной калейдоскоп
// ═══════════════════════════════════════════════════════════════════

pub unsafe fn kaleidoscope() {
    let fw = framebuffer::width();
    let fh = framebuffer::height();
    let cx = fw as f32 / 2.0;
    let cy = fh as f32 / 2.0;
    let max_r = (cx.max(cy)) as f32;

    let mut t = 0.0f32;
    loop {
        while let Some(k) = try_key() {
            if k == 0x1B { framebuffer::clear(Rgb(0,0,0)); return; }
        }

        for y in (0..fh).step_by(2) {
            for x in (0..fw).step_by(2) {
                let dx = (x as f32 - cx) / max_r;
                let dy = (y as f32 - cy) / max_r;
                let dist = crate::math::sqrt(dx * dx + dy * dy);
                let ang = crate::math::atan2(dy, dx);

                let half_pi = crate::math::PI / 2.0;
                let ka = ((ang * 4.0).abs() % half_pi).min(half_pi - 0.001);
                let kx = dist * crate::math::cos(ka);
                let ky = dist * crate::math::sin(ka);

                let r = (crate::math::sin(kx * 12.0 + t) * 127.0 + 128.0) as u8;
                let g = (crate::math::sin(ky * 12.0 + t * 1.3) * 127.0 + 128.0) as u8;
                let b = (crate::math::sin((kx + ky) * 8.0 + t * 0.7) * 127.0 + 128.0) as u8;

                framebuffer::fill_rect(x, y, 2, 2, Rgb(r, g, b));
            }
        }

        t += 0.04;
    }
}

// ═══════════════════════════════════════════════════════════════════
// Демо-раннер
// ═══════════════════════════════════════════════════════════════════

/// `demoloop` — запускает все демки по кругу.
pub unsafe fn demoloop() {
    crate::terminal::write(b"Demo loop: press any key to start...\n");
    wait_key();

    loop {
        crate::terminal::write(b"\n-- BOUNCE --\n");
        bounce();
        crate::terminal::write(b"\n-- FIRE --\n");
        fire();
        crate::terminal::write(b"\n-- MATRIX --\n");
        matrix_rain();
        crate::terminal::write(b"\n-- MANDELBROT --\n");
        mandelbrot();
        crate::terminal::write(b"\n-- STARFIELD --\n");
        starfield();
        crate::terminal::write(b"\n-- RAIN --\n");
        rain();
        crate::terminal::write(b"\n-- SNOW --\n");
        snow();
        crate::terminal::write(b"\n-- KALEIDOSCOPE --\n");
        kaleidoscope();
        crate::terminal::write(b"\n-- GAME OF LIFE --\n");
        crate::gfx3d::gol();
        crate::terminal::write(b"\n-- CUBE 3D --\n");
        crate::gfx3d::cube3d();
        crate::terminal::write(b"\n-- DONUT 3D --\n");
        crate::gfx3d::donut();
        crate::terminal::write(b"\n-- PLASMA --\n");
        crate::gfx3d::plasma();
        crate::terminal::write(b"\n-- TUNNEL --\n");
        crate::gfx3d::tunnel();
        crate::terminal::write(b"\n-- SNAKE --\n");
        play_snake();

        crate::terminal::write(b"\nDemo loop complete. Play again? [Y/n] ");
        let k = wait_key();
        if k == b'n' || k == b'N' || k == 0x1B { break; }
    }

    crate::terminal::clear();
    crate::terminal::write(b"Back to shell.\n");
}
