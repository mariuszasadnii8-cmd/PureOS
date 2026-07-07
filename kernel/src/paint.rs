//! Mini Paint — простая рисовалка в ядре PureOS.
//! Рисование мышкой: LMB рисует, RMB — ластик, колесо — размер кисти.
//! Escape — выход, C — очистить, 1-8 — цвета.
//!
//! Zero-Alloc: вся логика на статических буферах.

use crate::framebuffer::{self, Rgb};
use crate::keyboard;
use crate::terminal;

const PAL_H: u32 = 48;
const BTN_W: u32 = 32;
const BTN_H: u32 = 28;
const TITLE_H: u32 = 24;

struct PaintState {
    fg: Rgb,
    bg: Rgb,
    brush: u32,
    prev_mx: i32,
    prev_my: i32,
    drawing: bool,
    erasing: bool,
}

static PALETTE: [Rgb; 12] = [
    Rgb(0, 0, 0),       // 0: black
    Rgb(255, 255, 255), // 1: white
    Rgb(255, 50, 50),   // 2: red
    Rgb(255, 165, 0),   // 3: orange
    Rgb(255, 255, 50),  // 4: yellow
    Rgb(50, 255, 50),   // 5: green
    Rgb(50, 200, 255),  // 6: cyan
    Rgb(100, 100, 255), // 7: blue
    Rgb(200, 50, 255),  // 8: purple
    Rgb(150, 100, 50),  // 9: brown
    Rgb(200, 200, 200), // 10: light gray
    Rgb(80, 80, 80),    // 11: dark gray
];

pub unsafe fn run() {
    if framebuffer::width() == 0 || framebuffer::height() == 0 {
        terminal::write(b"paint: no framebuffer\n");
        return;
    }

    crate::usb::mouse_init();
    crate::usb::mouse_show();

    let fb_w = framebuffer::width();
    let fb_h = framebuffer::height();
    let canvas_h = fb_h.saturating_sub(PAL_H);

    framebuffer::fill_rect(0, 0, fb_w, fb_h, Rgb(255, 255, 255));
    draw_palette(fb_w, fb_h);
    draw_toolbar(fb_w, fb_h);

    let mut st = PaintState {
        fg: Rgb(0, 0, 0),
        bg: Rgb(255, 255, 255),
        brush: 4,
        prev_mx: -1,
        prev_my: -1,
        drawing: false,
        erasing: false,
    };

    loop {
        crate::usb::poll();
        crate::ps2mouse::poll();
        crate::usb::mouse_poll();

        let (mx, my) = crate::usb::mouse_pos();
        let buttons = crate::usb::mouse_buttons();

        // Keyboard input
        while let Some(ch) = crate::usb::key_read() {
            match ch {
                0x1B => { // Escape — exit
                    framebuffer::fill_rect(0, 0, fb_w, fb_h, Rgb(16, 16, 28));
                    terminal::clear();
                    terminal::write(b"paint exited\n");
                    return;
                }
                b'c' | b'C' => {
                    framebuffer::fill_rect(0, TITLE_H, fb_w, canvas_h, Rgb(255, 255, 255));
                }
                b'0'..=b'9' => {
                    let idx = (ch - b'0') as usize;
                    if idx < PALETTE.len() {
                        st.fg = PALETTE[idx];
                        st.brush = 4;
                    }
                }
                b'=' | b'+' => st.brush = (st.brush + 2).min(32),
                b'-' | b'_' => st.brush = st.brush.saturating_sub(2).max(1),
                _ => {}
            }
        }
        keyboard::poll();
        while let Some(ch) = keyboard::read_key() {
            match ch {
                0x1B => {
                    framebuffer::fill_rect(0, 0, fb_w, fb_h, Rgb(16, 16, 28));
                    terminal::clear();
                    terminal::write(b"paint exited\n");
                    return;
                }
                b'c' | b'C' => {
                    framebuffer::fill_rect(0, TITLE_H, fb_w, canvas_h, Rgb(255, 255, 255));
                }
                b'0'..=b'9' => {
                    let idx = (ch - b'0') as usize;
                    if idx < PALETTE.len() {
                        st.fg = PALETTE[idx];
                        st.brush = 4;
                    }
                }
                _ => {}
            }
        }

        // Проверка кликов на палитре
        let sep_y = fb_h - PAL_H;
        if my >= sep_y as i32 + TITLE_H as i32 + 4 {
            let rel_y = my - (sep_y as i32 + TITLE_H as i32 + 4);
            let col_idx = mx / (BTN_W as i32 + 4);
            if col_idx >= 0 && (col_idx as usize) < PALETTE.len() {
                let color_y = btns_per_row(fb_w);
                let row = rel_y / (BTN_H as i32 + 4);
                let idx = row * color_y + col_idx;
                if idx >= 0 && (idx as usize) < PALETTE.len() {
                    if buttons & 0x01 != 0 {
                        st.fg = PALETTE[idx as usize];
                    } else if buttons & 0x02 != 0 {
                        st.bg = PALETTE[idx as usize];
                    }
                }
            }
        }

        // Рисование на канвасе
        if my < sep_y as i32 && my >= TITLE_H as i32 {
            if buttons & 0x01 != 0 {
                if !st.drawing {
                    st.drawing = true;
                    st.prev_mx = mx;
                    st.prev_my = my;
                }
                draw_line(st.prev_mx, st.prev_my, mx, my, st.fg, st.brush);
                st.prev_mx = mx;
                st.prev_my = my;
            } else {
                st.drawing = false;
                st.prev_mx = -1;
                st.prev_my = -1;
            }

            if buttons & 0x02 != 0 {
                st.erasing = true;
                draw_line(mx, my, mx + 1, my + 1, st.bg, st.brush.max(8));
            } else {
                st.erasing = false;
            }
        }

        // Обновить хинт с текущим цветом и размером
        draw_palette(fb_w, fb_h);
        draw_hint(fb_w, fb_h, st.fg, st.brush);

        for _ in 0..20000 { core::hint::spin_loop(); }
    }
}

unsafe fn btns_per_row(fb_w: u32) -> i32 {
    ((fb_w - 10) / (BTN_W + 4)) as i32
}

unsafe fn draw_palette(fb_w: u32, fb_h: u32) {
    let sep_y = fb_h - PAL_H;
    framebuffer::fill_rect(0, sep_y, fb_w, PAL_H, Rgb(240, 240, 240));
    for x in 0..fb_w {
        framebuffer::put(x, sep_y, Rgb(180, 180, 180));
    }

    let npb = btns_per_row(fb_w);
    if npb <= 0 { return; }
    for (i, &color) in PALETTE.iter().enumerate() {
        let col = i as i32 % npb;
        let row = i as i32 / npb;
        let bx = 5 + col * (BTN_W as i32 + 4);
        let by = sep_y as i32 + 4 + row * (BTN_H as i32 + 4);
        framebuffer::fill_rect(bx as u32, by as u32, BTN_W, BTN_H, color);
        // border
        for dx in 0..BTN_W {
            framebuffer::put(bx as u32 + dx, by as u32, Rgb(100, 100, 100));
            framebuffer::put(bx as u32 + dx, by as u32 + BTN_H - 1, Rgb(100, 100, 100));
        }
        for dy in 0..BTN_H {
            framebuffer::put(bx as u32, by as u32 + dy, Rgb(100, 100, 100));
            framebuffer::put(bx as u32 + BTN_W - 1, by as u32 + dy, Rgb(100, 100, 100));
        }
    }
}

unsafe fn draw_toolbar(fb_w: u32, fb_h: u32) {
    framebuffer::fill_rect(0, 0, fb_w, TITLE_H, Rgb(220, 220, 220));
    for x in 0..fb_w {
        framebuffer::put(x, TITLE_H, Rgb(160, 160, 160));
    }

    let msg = b"  PAINT  [C]lear  [1-9]Color  [+/-]Brush  [Esc]Exit";
    let mut lx = 8u32;
    for &ch in msg {
        framebuffer::draw_char_boot(lx, 5, ch, Rgb(20, 20, 20), 1);
        lx += 8;
    }
}

unsafe fn draw_hint(fb_w: u32, fb_h: u32, fg: Rgb, brush: u32) {
    let sep_y = fb_h - PAL_H;
    let right_x = fb_w - 160;
    framebuffer::fill_rect(right_x, sep_y + 2, 158, PAL_H - 4, Rgb(240, 240, 240));

    let size_str = [b'B', b':', b' ', b'0' + (brush / 10) as u8, b'0' + (brush % 10) as u8];
    let mut lx = right_x + 4;
    for &ch in &size_str {
        framebuffer::draw_char_boot(lx, sep_y + 6, ch, Rgb(60, 60, 60), 1);
        lx += 8;
    }

    // Current color swatch
    framebuffer::fill_rect(right_x + 4, sep_y + 20, 30, 20, fg);
}

unsafe fn draw_line(x1: i32, y1: i32, x2: i32, y2: i32, color: Rgb, brush: u32) {
    let fb_w = framebuffer::width() as i32;
    let fb_h = framebuffer::height() as i32;
    let sep_y = fb_h - PAL_H as i32;
    let x1 = x1.max(TITLE_H as i32).min(fb_w - 1);
    let y1 = y1.max(TITLE_H as i32).min(sep_y - 1);
    let x2 = x2.max(TITLE_H as i32).min(fb_w - 1);
    let y2 = y2.max(TITLE_H as i32).min(sep_y - 1);

    let dx = (x2 - x1).abs();
    let dy = -(y2 - y1).abs();
    let sx = if x1 < x2 { 1 } else { -1 };
    let sy = if y1 < y2 { 1 } else { -1 };
    let mut err = dx + dy;

    let mut x = x1;
    let mut y = y1;
    loop {
        draw_dot(x, y, color, brush);
        if x == x2 && y == y2 { break; }
        let e2 = 2 * err;
        if e2 >= dy { err += dy; x += sx; }
        if e2 <= dx { err += dx; y += sy; }
    }
}

unsafe fn draw_dot(cx: i32, cy: i32, color: Rgb, brush: u32) {
    let fb_w = framebuffer::width() as i32;
    let fb_h = framebuffer::height() as i32;
    let sep_y = fb_h - PAL_H as i32;
    let r = (brush / 2) as i32;
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r * r {
                let px = cx + dx;
                let py = cy + dy;
                if px >= 0 && px < fb_w && py >= TITLE_H as i32 && py < sep_y {
                    framebuffer::put(px as u32, py as u32, color);
                }
            }
        }
    }
}
