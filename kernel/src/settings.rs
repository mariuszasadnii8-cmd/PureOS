//! PureOS Settings — графическая панель настроек.
//! Framebuffer-окно: выбор обоев, цвета терминала, масштаб шрифта.
//! Zero-Alloc: статические буферы.

use crate::framebuffer::{self, Rgb};
use crate::font::FontId;
use crate::terminal;

const PANEL_W: u32 = 520;
const PANEL_H: u32 = 400;
const ITEM_H: u32 = 32;

static mut SELECTED_ITEM: usize = 0;

enum SettingsAction {
    Wallpaper,
    FontScale,
    Colors,
    Mouse,
    SystemInfo,
    Back,
}

static SETTINGS_ITEMS: [(SettingsAction, &[u8]); 6] = [
    (SettingsAction::Wallpaper,  b"Wallpaper"),
    (SettingsAction::FontScale,  b"Font Scale"),
    (SettingsAction::Colors,     b"Terminal Colors"),
    (SettingsAction::Mouse,      b"Mouse Sensitivity"),
    (SettingsAction::SystemInfo, b"System Info"),
    (SettingsAction::Back,       b"Back to Desktop"),
];

pub unsafe fn run() {
    if framebuffer::width() == 0 || framebuffer::height() == 0 {
        terminal::write(b"settings: no framebuffer\n");
        return;
    }

    loop {
        let fb_w = framebuffer::width();
        let fb_h = framebuffer::height();
        let px = (fb_w - PANEL_W) / 2;
        let py = (fb_h - PANEL_H) / 3;

        draw_panel(px, py, PANEL_W, PANEL_H);
        draw_header(px, py, PANEL_W, b"  Settings");

        let list_y = py + 44;
        for i in 0..SETTINGS_ITEMS.len() {
            let yp = list_y + i as u32 * ITEM_H;
            let bg = if i == SELECTED_ITEM { Rgb(50, 70, 130) } else { Rgb(28, 28, 38) };
            fill_rect(px + 4, yp, PANEL_W - 8, ITEM_H - 2, bg);
            let label = SETTINGS_ITEMS[i].1;
            let mut lx = px + 12;
            for &ch in label {
                framebuffer::draw_char(lx, yp + 6, ch, Rgb(210, 210, 230), 1, FontId::Compact);
                lx += 8;
            }
        }

        let hint_y = py + PANEL_H - 20;
        hline(px + 4, hint_y - 4, PANEL_W - 8, Rgb(50, 50, 60));
        let hint = b"Up/Down=select  Enter=open  Esc=back";
        let mut hx = px + 10;
        for &ch in hint {
            framebuffer::draw_char(hx, hint_y, ch, Rgb(160, 160, 180), 1, FontId::Compact);
            hx += 8;
        }

        crate::usb::mouse_init();
        crate::usb::mouse_show();

        'input: loop {
            crate::usb::poll();
            crate::ps2mouse::poll();
            crate::usb::mouse_poll();

            while let Some(ch) = crate::usb::key_read() {
                match ch {
                    0x1B => { return; }
                    b'\n' | b'\r' => {
                        match SETTINGS_ITEMS[SELECTED_ITEM].0 {
                            SettingsAction::Wallpaper => { wallpaper_chooser(); break 'input; }
                            SettingsAction::FontScale => { font_scale_chooser(); break 'input; }
                            SettingsAction::Colors => { color_chooser(); break 'input; }
                            SettingsAction::Mouse => { mouse_sensitivity_chooser(); break 'input; }
                            SettingsAction::SystemInfo => { system_info_screen(); break 'input; }
                            SettingsAction::Back => { return; }
                        }
                    }
                    b'w' | b'W' | 0x1E => {
                        if SELECTED_ITEM > 0 { SELECTED_ITEM -= 1; break 'input; }
                    }
                    b's' | b'S' | 0x1F => {
                        if SELECTED_ITEM + 1 < SETTINGS_ITEMS.len() { SELECTED_ITEM += 1; break 'input; }
                    }
                    _ => {}
                }
            }

            let (mx, my) = crate::usb::mouse_pos();
            let buttons = crate::usb::mouse_buttons();
            if buttons & 0x01 != 0 {
                let rel_x = mx - px as i32;
                let rel_y = my - list_y as i32;
                if rel_x >= 0 && rel_x < PANEL_W as i32 && rel_y >= 0 {
                    let idx = rel_y as u32 / ITEM_H;
                    if (idx as usize) < SETTINGS_ITEMS.len() {
                        SELECTED_ITEM = idx as usize;
                        match SETTINGS_ITEMS[SELECTED_ITEM].0 {
                            SettingsAction::Wallpaper => { wallpaper_chooser(); break 'input; }
                            SettingsAction::FontScale => { font_scale_chooser(); break 'input; }
                            SettingsAction::Colors => { color_chooser(); break 'input; }
                            SettingsAction::Mouse => { mouse_sensitivity_chooser(); break 'input; }
                            SettingsAction::SystemInfo => { system_info_screen(); break 'input; }
                            SettingsAction::Back => { return; }
                        }
                    }
                }
            }

            core::arch::asm!("pause", options(nomem, nostack, preserves_flags));
        }
    }
}

unsafe fn wallpaper_chooser() {
    let names: [&[u8]; 10] = [
        b"waves", b"solid", b"gradient", b"stripes", b"checkers",
        b"radial", b"grid", b"noise", b"bg1", b"bg2",
    ];
    let fb_w = framebuffer::width();
    let fb_h = framebuffer::height();

    loop {
        // Показать сетку превью
        let cols = 4u32;
        let preview_w = 120u32;
        let preview_h = 80u32;
        let gap = 12u32;
        let grid_w = cols * (preview_w + gap) - gap;
        let start_x = (fb_w - grid_w) / 2;
        let start_y = fb_h / 6;

        // Фон
        fill_rect(0, 0, fb_w, fb_h, Rgb(10, 15, 25));

        for i in 0..10 {
            let col = i as u32 % cols;
            let row = i as u32 / cols;
            let px = start_x + col * (preview_w + gap);
            let py = start_y + row * (preview_h + gap + 24);

            // Рамка
            let sel = if i as usize == SELECTED_ITEM % 10 { Rgb(255, 200, 50) } else { Rgb(80, 80, 100) };
            for dx in 0..preview_w { framebuffer::put(px + dx, py, sel); framebuffer::put(px + dx, py + preview_h - 1, sel); }
            for dy in 0..preview_h { framebuffer::put(px, py + dy, sel); framebuffer::put(px + preview_w - 1, py + dy, sel); }

            // Подпись
            let label = names[i];
            let lx = px + (preview_w - (label.len() as u32 * 8)) / 2;
            for (j, &ch) in label.iter().enumerate() {
                framebuffer::draw_char(lx + j as u32 * 8, py + preview_h + 4, ch, Rgb(210, 210, 230), 1, FontId::Compact);
            }
        }

        let hint = b"Click to select  Esc=cancel";
        let hh = fb_h - 30;
        let hx = (fb_w - (hint.len() as u32 * 8)) / 2;
        for (i, &ch) in hint.iter().enumerate() {
            framebuffer::draw_char(hx + i as u32 * 8, hh, ch, Rgb(160, 160, 180), 1, FontId::Compact);
        }

        crate::usb::mouse_show();

        loop {
            crate::usb::poll();
            crate::ps2mouse::poll();
            crate::usb::mouse_poll();

            while let Some(ch) = crate::usb::key_read() {
                match ch {
                    0x1B => { return; }
                    b'\n' | b'\r' => {
                        let idx = SELECTED_ITEM % 10;
                        crate::wallpaper::set_wallpaper_by_name(names[idx]);
                        return;
                    }
                    _ => {}
                }
            }

            let (mx, my) = crate::usb::mouse_pos();
            let buttons = crate::usb::mouse_buttons();
            if buttons & 0x01 != 0 {
                for i in 0..10 {
                    let col = i as u32 % cols;
                    let row = i as u32 / cols;
                    let px2 = start_x + col * (preview_w + gap);
                    let py2 = start_y + row * (preview_h + gap + 24);
                    if mx >= px2 as i32 && mx < (px2 + preview_w) as i32
                        && my >= py2 as i32 && my < (py2 + preview_h) as i32
                    {
                        SELECTED_ITEM = i;
                        crate::wallpaper::set_wallpaper_by_name(names[i]);
                        return;
                    }
                }
            }

            core::arch::asm!("pause", options(nomem, nostack, preserves_flags));
        }
    }
}

unsafe fn font_scale_chooser() {
    let fb_w = framebuffer::width();
    let fb_h = framebuffer::height();
    let mut scale = crate::config::get_font_scale();

    loop {
        fill_rect(0, 0, fb_w, fb_h, Rgb(10, 15, 25));
        let title = b"Font Scale";
        let tx = (fb_w - (title.len() as u32 * 8)) / 2;
        for (i, &ch) in title.iter().enumerate() {
            framebuffer::draw_char(tx + i as u32 * 8, fb_h / 4, ch, Rgb(200, 200, 220), 1, FontId::Compact);
        }

        let val_str = [b'0' + (scale / 10) as u8, b'0' + (scale % 10) as u8];
        let vx = (fb_w - 16) / 2;
        for (i, &ch) in val_str.iter().enumerate() {
            framebuffer::draw_char(vx + i as u32 * 8, fb_h / 3, ch, Rgb(255, 255, 255), 4, FontId::Compact);
        }

        let hint = b"Up/Down=change  Enter=save  Esc=cancel";
        let hx = (fb_w - (hint.len() as u32 * 8)) / 2;
        for (i, &ch) in hint.iter().enumerate() {
            framebuffer::draw_char(hx + i as u32 * 8, fb_h * 3 / 4, ch, Rgb(160, 160, 180), 1, FontId::Compact);
        }

        crate::usb::mouse_show();

        loop {
            crate::usb::poll();
            crate::usb::mouse_poll();
            while let Some(ch) = crate::usb::key_read() {
                match ch {
                    0x1B => { return; }
                    b'\n' | b'\r' => {
                        crate::config::set_font_scale(scale);
                        return;
                    }
                    b'w' | b'W' | 0x1E => { if scale < 9 { scale += 1; } break; }
                    b's' | b'S' | 0x1F => { if scale > 1 { scale -= 1; } break; }
                    _ => {}
                }
            }
            core::arch::asm!("pause", options(nomem, nostack, preserves_flags));
        }
    }
}

unsafe fn color_chooser() {
    let fb_w = framebuffer::width();
    let fb_h = framebuffer::height();
    let mut colors = crate::config::get_terminal_colors();

    let presets: [(&[u8], u8, u8, u8, u8, u8, u8); 6] = [
        (b"Default",   0, 0, 0, 255, 255, 255),
        (b"Light",     255, 255, 255, 0, 0, 0),
        (b"Amber",     0, 0, 0, 255, 191, 0),
        (b"Green",     0, 0, 0, 0, 255, 0),
        (b"Ocean",     0, 10, 20, 100, 200, 230),
        (b"Matrix",    0, 0, 0, 0, 200, 0),
    ];

    loop {
        fill_rect(0, 0, fb_w, fb_h, Rgb(10, 15, 25));
        let title = b"Terminal Colors";
        let tx = (fb_w - (title.len() as u32 * 8)) / 2;
        for (i, &ch) in title.iter().enumerate() {
            framebuffer::draw_char(tx + i as u32 * 8, 40, ch, Rgb(200, 200, 220), 1, FontId::Compact);
        }

        for (i, (name, bgr, bgg, bgb, fgr, fgg, fgb)) in presets.iter().enumerate() {
            let yp = 80 + i as u32 * 28;
            let label_x = 60u32;
            // BG preview
            for dx in 0..20 { for dy in 0..16 { framebuffer::put(label_x - 24 + dx, yp + dy, Rgb(*bgr, *bgg, *bgb)); } }
            // FG preview
            let ch = b'A';
            framebuffer::draw_char(label_x - 20, yp + 3, ch, Rgb(*fgr, *fgg, *fgb), 1, FontId::Compact);
            // Label
            for (j, &c) in name.iter().enumerate() {
                framebuffer::draw_char(label_x + j as u32 * 8 + 8, yp + 2, c, Rgb(210, 210, 230), 1, FontId::Compact);
            }
        }

        let hint = b"Click preset  Esc=cancel";
        let hx = (fb_w - (hint.len() as u32 * 8)) / 2;
        for (i, &ch) in hint.iter().enumerate() {
            framebuffer::draw_char(hx + i as u32 * 8, fb_h - 30, ch, Rgb(160, 160, 180), 1, FontId::Compact);
        }

        crate::usb::mouse_show();

        loop {
            crate::usb::poll();
            crate::ps2mouse::poll();
            crate::usb::mouse_poll();
            while let Some(ch) = crate::usb::key_read() {
                if ch == 0x1B { return; }
            }
            let (mx, my) = crate::usb::mouse_pos();
            let buttons = crate::usb::mouse_buttons();
            if buttons & 0x01 != 0 {
                for (i, (_, bgr, bgg, bgb, fgr, fgg, fgb)) in presets.iter().enumerate() {
                    let yp = 80 + i as u32 * 28;
                    if mx >= 40 && mx < 300 && my >= yp as i32 && my < (yp + 20) as i32 {
                        colors.background_r = *bgr;
                        colors.background_g = *bgg;
                        colors.background_b = *bgb;
                        colors.foreground_r = *fgr;
                        colors.foreground_g = *fgg;
                        colors.foreground_b = *fgb;
                        crate::config::set_terminal_colors(colors);
                        terminal::apply_colors_from_config();
                        return;
                    }
                }
            }
            core::arch::asm!("pause", options(nomem, nostack, preserves_flags));
        }
    }
}

unsafe fn mouse_sensitivity_chooser() {
    let fb_w = framebuffer::width();
    let fb_h = framebuffer::height();
    let mut sens = crate::config::get_mouse_sensitivity();

    loop {
        fill_rect(0, 0, fb_w, fb_h, Rgb(10, 15, 25));
        let title = b"Mouse Sensitivity";
        let tx = (fb_w - (title.len() as u32 * 8)) / 2;
        for (i, &ch) in title.iter().enumerate() {
            framebuffer::draw_char(tx + i as u32 * 8, fb_h / 4, ch, Rgb(200, 200, 220), 1, FontId::Compact);
        }

        let val_str = [b'0' + (sens) as u8];
        let vx = (fb_w - 8) / 2;
        framebuffer::draw_char(vx, fb_h / 3, val_str[0], Rgb(255, 255, 255), 4, FontId::Compact);

        // Visual bar
        let bar_x = fb_w / 2 - 80;
        let bar_y = fb_h / 3 + 40;
        let bar_w = 160u32;
        for dx in 0..bar_w {
            let c = if dx < (sens * bar_w / 9) { Rgb(100, 200, 255) } else { Rgb(40, 40, 50) };
            for dy in 0..8 { framebuffer::put(bar_x + dx, bar_y + dy, c); }
        }
        hline(bar_x, bar_y + 10, bar_w, Rgb(50, 50, 60));

        let hint = b"Up/Down=change  Enter=save  Esc=cancel";
        let hx = (fb_w - (hint.len() as u32 * 8)) / 2;
        for (i, &ch) in hint.iter().enumerate() {
            framebuffer::draw_char(hx + i as u32 * 8, fb_h * 3 / 4, ch, Rgb(160, 160, 180), 1, FontId::Compact);
        }

        crate::usb::mouse_show();

        loop {
            crate::usb::poll();
            crate::ps2mouse::poll();
            crate::usb::mouse_poll();
            while let Some(ch) = crate::usb::key_read() {
                match ch {
                    0x1B => { return; }
                    b'\n' | b'\r' => {
                        crate::config::set_mouse_sensitivity(sens);
                        return;
                    }
                    b'w' | b'W' | 0x1E => { if sens < 9 { sens += 1; } break; }
                    b's' | b'S' | 0x1F => { if sens > 1 { sens -= 1; } break; }
                    _ => {}
                }
            }
            core::arch::asm!("pause", options(nomem, nostack, preserves_flags));
        }
    }
}

unsafe fn system_info_screen() {
    let fb_w = framebuffer::width();
    let fb_h = framebuffer::height();

    loop {
        fill_rect(0, 0, fb_w, fb_h, Rgb(10, 15, 25));
        let title = b"System Info";
        let tx = (fb_w - (title.len() as u32 * 8)) / 2;
        for (i, &ch) in title.iter().enumerate() {
            framebuffer::draw_char(tx + i as u32 * 8, 30, ch, Rgb(200, 200, 220), 1, FontId::Compact);
        }

        let lines: &[&[u8]] = &[
            b"PureOS v0.4 - Immutable Ephemeral Kernel",
            b"",
            b"CPU: x86_64 AMD64 (qemu64)",
            b"Kernel: zero-alloc, no heap",
            b"Heap: bump frame allocator (64 MiB pool)",
            b"Processes: 64 max, round-robin + APIC timer",
            b"IPC: synchronous rendezvous",
            b"",
            b"Display: framebuffer 32bpp",
            b"Input: PS/2 (i8042) + USB HID",
            b"Storage: ATA PIO + blockfs",
            b"",
            b"Press any key or click to return",
        ];

        let mut yp = 80u32;
        for line in lines {
            let lx = (fb_w - (line.len() as u32 * 8)) / 2;
            for (j, &ch) in line.iter().enumerate() {
                framebuffer::draw_char(lx + j as u32 * 8, yp, ch, Rgb(180, 200, 220), 1, FontId::Compact);
            }
            yp += 18;
        }

        crate::usb::mouse_show();

        loop {
            crate::usb::poll();
            crate::ps2mouse::poll();
            crate::usb::mouse_poll();
            while let Some(_) = crate::usb::key_read() {
                return;
            }
            let buttons = crate::usb::mouse_buttons();
            if buttons & 0x01 != 0 { return; }
            core::arch::asm!("pause", options(nomem, nostack, preserves_flags));
        }
    }
}

// ── Drawing helpers ──

unsafe fn draw_panel(x: u32, y: u32, w: u32, h: u32) {
    for dy in 0..h {
        let ay = y + dy;
        if ay >= framebuffer::height() { break; }
        for dx in 0..w {
            let ax = x + dx;
            if ax >= framebuffer::width() { break; }
            framebuffer::put(ax, ay, Rgb(25, 25, 35));
        }
    }
    for ix in x..x+w { framebuffer::put(ix, y, Rgb(100, 130, 200)); }
    for ix in x..x+w { framebuffer::put(ix, y+h-1, Rgb(50, 60, 110)); }
    for iy in y..y+h { framebuffer::put(x, iy, Rgb(100, 130, 200)); }
    for iy in y..y+h { framebuffer::put(x+w-1, iy, Rgb(50, 60, 110)); }
}

unsafe fn draw_header(x: u32, y: u32, w: u32, title: &[u8]) {
    fill_rect(x+1, y+1, w-2, 28, Rgb(35, 45, 75));
    let mut lx = x + 8;
    for &ch in title {
        framebuffer::draw_char(lx, y + 6, ch, Rgb(210, 210, 230), 1, FontId::Compact);
        lx += 8;
    }
}

unsafe fn fill_rect(x: u32, y: u32, w: u32, h: u32, c: Rgb) {
    for dy in 0..h {
        let ay = y + dy;
        if ay >= framebuffer::height() { break; }
        for dx in 0..w {
            let ax = x + dx;
            if ax >= framebuffer::width() { break; }
            framebuffer::put(ax, ay, c);
        }
    }
}

unsafe fn hline(x: u32, y: u32, w: u32, c: Rgb) {
    for dx in 0..w {
        let ax = x + dx;
        if ax >= framebuffer::width() { break; }
        framebuffer::put(ax, y, c);
    }
}
