//! Простой файловый менеджер (проводник) для рабочего стола.
//!
//! Рисует окно со списком файлов текущего каталога.
//! Enter / клик — открыть (каталог = войти, .elf/.pos = exec).
//! Escape — назад / выход.

use crate::framebuffer::{self, Rgb};
use crate::font::FontId;
use crate::terminal;

const ITEM_H: u32 = 28;

static mut CWD: u16 = crate::fs::ROOT;
static mut SCROLL: usize = 0;
static mut SELECTED: usize = 0;
static mut FILE_COUNT: usize = 0;
static mut FILE_LIST: [u16; 256] = [0; 256];
static mut PATH_BUF: [u8; 256] = [0; 256];
static mut PATH_LEN: usize = 0;

/// Запустить файловый менеджер (framebuffer-режим).
pub unsafe fn run() {
    if framebuffer::width() == 0 || framebuffer::height() == 0 {
        terminal::write(b"filer: no framebuffer\n");
        return;
    }
    CWD = crate::fs::ROOT;
    SCROLL = 0;
    SELECTED = 0;
    update_path();
    refresh_list();

    loop {
        let w = framebuffer::width();
        let h = framebuffer::height();
        let win_x = w / 8;
        let win_y = h / 10;
        let win_w = w * 3 / 4;
        let win_h = h * 4 / 5;

        draw_panel(win_x, win_y, win_w, win_h);
        draw_header(win_x, win_y, win_w);

        // Путь
        let py = win_y + 38;
        draw_text(win_x + 10, py, &PATH_BUF[..PATH_LEN]);
        draw_hline(win_x + 4, py + 18, win_w - 8, Rgb(80, 80, 100));

        // Список файлов — три колонки: Type | Name | Size
        let list_top = py + 28;
        let avail_h = win_h - (list_top - win_y) - 8;
        let visible = (avail_h / ITEM_H) as usize;
        let col_type = win_x + 12;
        let col_name = win_x + 64;
        let col_size = win_x + win_w - 80;

        // Заголовок колонок
        let hdr_y = list_top - 18;
        draw_text(col_type, hdr_y, b"Type");
        draw_text(col_name, hdr_y, b"Name");
        draw_text(col_size, hdr_y, b"Size");

        for i in 0..visible {
            let idx = SCROLL + i;
            if idx >= FILE_COUNT { break; }
            let node = FILE_LIST[idx];
            let name = crate::fs::node_name(node);
            let kind = crate::fs::kind(node);
            let ypos = list_top + i as u32 * ITEM_H;

            let bg = if idx == SELECTED { Rgb(50, 70, 130) } else { Rgb(28, 28, 38) };
            draw_rect(win_x + 4, ypos, win_w - 8, ITEM_H - 2, bg);

            // Колонка Type
            let icon = match kind {
                crate::fs::Kind::Dir => b"[DIR]",
                crate::fs::Kind::File => {
                    if name.len() >= 4 && name[name.len()-4..].eq_ignore_ascii_case(b".pos") {
                        b"[POS]"
                    } else if name.len() >= 4 && name[name.len()-4..].eq_ignore_ascii_case(b".elf") {
                        b"[ELF]"
                    } else {
                        b"[TXT]"
                    }
                }
                _ => b"[?]  "
            };
            draw_text(col_type, ypos + 2, icon);

            // Колонка Name
            let max_n = if name.len() > 28 { 28 } else { name.len() };
            draw_text(col_name, ypos + 2, &name[..max_n]);

            // Колонка Size
            if let crate::fs::Kind::File = kind {
                let sz = crate::fs::size_of(node);
                let size_buf = format_size_short(sz);
                draw_text(col_size, ypos + 2, &size_buf);
            }
        }

        // Подсказка
        let hint_y = win_y + win_h - 20;
        draw_hline(win_x + 4, hint_y - 4, win_w - 8, Rgb(50, 50, 60));
        draw_text(win_x + 10, hint_y, b"W/S=nav  Enter=open  Esc=up  Q=exit");

        crate::usb::mouse_init();
        crate::usb::mouse_show();

        let mut redraw = false;
        'input: loop {
            crate::usb::poll();
            crate::ps2mouse::poll();
            crate::usb::mouse_poll();

            while let Some(ch) = crate::usb::key_read() {
                match ch {
                    0x1B => {
                        if CWD != crate::fs::ROOT {
                            go_up();
                            redraw = true;
                            break 'input;
                        }
                        break 'input;
                    }
                    b'\n' | b'\r' => {
                        if SELECTED < FILE_COUNT {
                            open_selected();
                            redraw = true;
                            break 'input;
                        }
                    }
                    b'q' | b'Q' => { break 'input; }
                    b'w' | b'W' => {
                        if SELECTED > 0 { SELECTED -= 1; if SELECTED < SCROLL { SCROLL = SELECTED; } redraw = true; break 'input; }
                    }
                    b's' | b'S' => {
                        if SELECTED + 1 < FILE_COUNT { SELECTED += 1; if SELECTED >= SCROLL + visible { SCROLL = SELECTED - visible + 1; } redraw = true; break 'input; }
                    }
                    _ => {}
                }
            }
            // Стрелки через PS/2 не проходят, поэтому игнорируем keyboard::read_key

            let (mx, my) = crate::usb::mouse_pos();
            let buttons = crate::usb::mouse_buttons();
            if buttons & 0x01 != 0 {
                let rel_x = mx - win_x as i32;
                let rel_y = my - list_top as i32;
                if rel_x >= 0 && rel_x < win_w as i32 && rel_y >= 0 {
                    let idx = rel_y as u32 / ITEM_H;
                    if (idx as usize) < visible && (SCROLL + idx as usize) < FILE_COUNT {
                        SELECTED = SCROLL + idx as usize;
                        open_selected();
                        redraw = true;
                        break 'input;
                    }
                }
            }

            core::arch::asm!("pause", options(nomem, nostack, preserves_flags));
        }

        if !redraw { break; }
    }
}

unsafe fn go_up() {
    if let Some(parent) = crate::fs::resolve_from(CWD, b"..") {
        CWD = parent;
        update_path();
        refresh_list();
        SCROLL = 0;
        SELECTED = 0;
    }
}

unsafe fn open_selected() {
    if SELECTED >= FILE_COUNT { return; }
    let node = FILE_LIST[SELECTED];
    let kind = crate::fs::kind(node);
    let name = crate::fs::node_name(node);

    if kind == crate::fs::Kind::Dir {
        CWD = node;
        update_path();
        refresh_list();
        SCROLL = 0;
        SELECTED = 0;
    } else if kind == crate::fs::Kind::File {
        if name.len() >= 4 && name[name.len()-4..].eq_ignore_ascii_case(b".pos") {
            let data = crate::fs::read(node);
            if data.len() >= 16 { crate::pos::exec(data); }
        } else if name.len() >= 4 && name[name.len()-4..].eq_ignore_ascii_case(b".elf") {
            let data = crate::fs::read(node);
            if !data.is_empty() { crate::elf::exec(data.as_ptr() as u64, data.len() as u64); }
        }
    }
}

unsafe fn refresh_list() {
    FILE_COUNT = 0;
    crate::fs::for_each_child(CWD, |idx| {
        if FILE_COUNT >= FILE_LIST.len() { return; }
        FILE_LIST[FILE_COUNT] = idx;
        FILE_COUNT += 1;
    });
}

unsafe fn update_path() {
    PATH_LEN = 0;
    let mut cur = CWD;
    // Собираем имена от CWD до ROOT
    let mut names: [&[u8]; 32] = [b""; 32];
    let mut np = 0usize;
    while cur != crate::fs::ROOT && np < names.len() {
        names[np] = crate::fs::node_name(cur);
        np += 1;
        cur = crate::fs::resolve_from(cur, b"..").unwrap_or(crate::fs::ROOT);
    }
    PATH_BUF[0] = b'/';
    PATH_LEN = 1;
    let mut i = np;
    while i > 0 {
        i -= 1;
        let name = names[i];
        if name.len() == 0 { continue; }
        if PATH_LEN + name.len() + 1 > PATH_BUF.len() { break; }
        for &b in name { PATH_BUF[PATH_LEN] = b; PATH_LEN += 1; }
        if i > 0 { PATH_BUF[PATH_LEN] = b'/'; PATH_LEN += 1; }
    }
    PATH_BUF[PATH_LEN] = 0;
}

// ── Drawing ──

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

unsafe fn draw_header(x: u32, y: u32, w: u32) {
    draw_rect(x+1, y+1, w-2, 28, Rgb(35, 45, 75));
    draw_text(x+8, y+6, b"  PureOS Explorer");
}

unsafe fn draw_rect(x: u32, y: u32, w: u32, h: u32, c: Rgb) {
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

unsafe fn draw_hline(x: u32, y: u32, w: u32, c: Rgb) {
    for dx in 0..w {
        let ax = x + dx;
        if ax >= framebuffer::width() { break; }
        framebuffer::put(ax, y, c);
    }
}

unsafe fn draw_text(mut x: u32, y: u32, s: &[u8]) {
    let fg = Rgb(210, 210, 230);
    for &ch in s {
        framebuffer::draw_char(x, y, ch, fg, 1, FontId::Compact);
        x += 8;
    }
}

/// Форматировать размер в короткое строковое представление (B/KB).
fn format_size_short(sz: u32) -> [u8; 8] {
    let mut buf = [b' '; 8];
    if sz < 1024 {
        let mut v = sz as u64;
        let mut i = 6;
        loop {
            buf[i] = b'0' + (v % 10) as u8;
            v /= 10;
            if v == 0 || i == 0 { break; }
            i -= 1;
        }
        buf[7] = b'B';
    } else {
        let kb = sz / 1024;
        let mut v = kb as u64;
        let mut i = 5;
        loop {
            buf[i] = b'0' + (v % 10) as u8;
            v /= 10;
            if v == 0 || i == 0 { break; }
            i -= 1;
        }
        buf[6] = b'K'; buf[7] = b'B';
    }
    buf
}
