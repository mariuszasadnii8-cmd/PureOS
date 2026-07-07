//! PureOS Desktop Environment — лёгкий графический режим.
//!
//! Дизайн: prompt.txt — док снизу по центру, пастельные тона,
//! скругления, мягкие тени.
//!
//! Zero-Alloc: статические буферы, все отрисовки напрямую в фреймбуфер.

use crate::framebuffer::{self, Rgb};
use crate::graphics;
use crate::keyboard;
use crate::terminal;
use crate::window;

// ── Palette (prompt.txt) ──
const BG: Rgb = Rgb(250, 250, 248);
const SURFACE: Rgb = Rgb(238, 245, 239);
const ACCENT: Rgb = Rgb(127, 168, 140);
const DARK_ACCENT: Rgb = Rgb(53, 94, 74);
const BORDER: Rgb = Rgb(217, 228, 220);
const TEXT_FG: Rgb = Rgb(40, 50, 44);
const TEXT_SEC: Rgb = Rgb(120, 130, 120);
const SHADOW: Rgb = Rgb(200, 210, 200);

// ── Встроенные иконки (raw 32bpp BGRA, 48×48, 9216 байт каждая) ──
static ICON_EXPLORER: &[u8]  = include_bytes!("../../userspace/personal/explorer.bin");
static ICON_PAINT: &[u8]     = include_bytes!("../../userspace/personal/paint.bin");
static ICON_REBOOT: &[u8]    = include_bytes!("../../userspace/personal/rebooticon.bin");
static ICON_SETTINGS: &[u8]  = include_bytes!("../../userspace/personal/settingsicon.bin");
static ICON_SNAKE: &[u8]     = include_bytes!("../../userspace/personal/snake.bin");
static ICON_TERMINAL: &[u8]  = include_bytes!("../../userspace/personal/shutdown.bin");

fn icon_data_for(label: &[u8]) -> &'static [u8] {
    match label {
        b"Files"     | b"Apps" => ICON_EXPLORER,
        b"Terminal"  => ICON_TERMINAL,
        b"Snake"     => ICON_SNAKE,
        b"Paint"     => ICON_PAINT,
        b"Settings"  => ICON_SETTINGS,
        b"Reboot"    => ICON_REBOOT,
        _            => &[],
    }
}

/// Нарисовать raw 32bpp BGRA иконку с произвольным квадратным размером,
/// растянутую на ICON_SIZE×ICON_SIZE (nearest-neighbour).
unsafe fn draw_raw_icon(x: u32, y: u32, data: &[u8]) {
    let dst = ICON_SIZE;
    let n = data.len() / 4; // пикселей
    if n == 0 { return; }
    let src = isqrt(n as u32);
    if src == 0 || (src * src) as usize != n { return; } // не квадрат — не рисуем
    for row in 0..dst {
        for col in 0..dst {
            let sr = (row * src) / dst;
            let sc = (col * src) / dst;
            let off = ((sr * src + sc) * 4) as usize;
            if off + 3 >= data.len() { continue; }
            let b = data[off];
            let g = data[off + 1];
            let r = data[off + 2];
            let a = data[off + 3];
            if a < 128 { continue; }
            framebuffer::put(x + col, y + row, Rgb(r, g, b));
        }
    }
}

/// Целочисленный квадратный корень (Newton).
const fn isqrt(mut n: u32) -> u32 {
    if n == 0 { return 0; }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

// ── Sizes ──
const DOCK_H: u32 = 64;
const DOCK_RADIUS: u32 = 24;
const ICON_SIZE: u32 = 48;
const ICON_GAP: u32 = 12;
const ICONS_Y: u32 = 32;
const MENU_RADIUS: u32 = 16;
const BTN_RADIUS: u32 = 12;

static mut CLOCK_HOUR: u32 = 0;
static mut CLOCK_MIN: u32 = 0;
static mut CLOCK_TICK: u64 = 0;
static mut CLOCK_INIT: bool = false;
static mut CLOCK_RTC_INIT: bool = false;

// ── Context menu ──
const MENU_W: u32 = 160;
const MENU_ITEM_H: u32 = 26;
const MENU_PAD: u32 = 6;

#[derive(Clone, Copy)]
struct MenuItem {
    label: &'static [u8],
    action: MenuAction,
}

#[derive(Clone, Copy, PartialEq)]
enum MenuAction {
    Terminal,
    Files,
    Snake,
    Paint,
    Apps,
    Settings,
    Wallpaper,
    Reboot,
    Shutdown,
    Properties,
    Cancel,
}

const CTX_MENU_ITEMS: [MenuItem; 10] = [
    MenuItem { label: b"Terminal",  action: MenuAction::Terminal },
    MenuItem { label: b"Files",     action: MenuAction::Files },
    MenuItem { label: b"Snake",     action: MenuAction::Snake },
    MenuItem { label: b"Paint",     action: MenuAction::Paint },
    MenuItem { label: b"Apps",      action: MenuAction::Apps },
    MenuItem { label: b"Settings",  action: MenuAction::Settings },
    MenuItem { label: b"Wallpaper", action: MenuAction::Wallpaper },
    MenuItem { label: b"---------", action: MenuAction::Cancel },
    MenuItem { label: b"Properties",action: MenuAction::Properties },
    MenuItem { label: b"Reboot",    action: MenuAction::Reboot },
];

// ── Start menu ──
const START_MENU_W: u32 = 180;
const START_MENUITEM_H: u32 = 32;
const START_RADIUS: u32 = 16;
static mut START_MENU_ACTIVE: bool = false;
static mut START_SEL: usize = 0;

struct StartItem {
    label: &'static [u8],
    action: MenuAction,
}

const START_ITEMS: [StartItem; 9] = [
    StartItem { label: b"Terminal",  action: MenuAction::Terminal },
    StartItem { label: b"Files",     action: MenuAction::Files },
    StartItem { label: b"Snake",     action: MenuAction::Snake },
    StartItem { label: b"Paint",     action: MenuAction::Paint },
    StartItem { label: b"Settings",  action: MenuAction::Settings },
    StartItem { label: b"Wallpaper", action: MenuAction::Wallpaper },
    StartItem { label: b"---------", action: MenuAction::Cancel },
    StartItem { label: b"Reboot",    action: MenuAction::Reboot },
    StartItem { label: b"Cancel",    action: MenuAction::Cancel },
];

// ── Menu background save buffer ──
const MENU_BG_W: usize = 220;
const MENU_BG_H: usize = 320;
static mut MENU_BG: [u32; MENU_BG_W * MENU_BG_H] = [0; MENU_BG_W * MENU_BG_H];
static mut MENU_SAVED_W: u32 = 0;
static mut MENU_SAVED_H: u32 = 0;
static mut MENU_SAVED_X: i32 = 0;
static mut MENU_SAVED_Y: i32 = 0;
static mut MENU_ACTIVE: bool = false;

// ── Встроенные данные иконок (raw 32bpp BGRA, 48×48) ──
const ICON_DATA_SIZE: usize = 9216; // 48×48×4

// ── Desktop icons ──
#[derive(Clone, Copy)]
struct DesktopIcon {
    x: u32,
    y: u32,
    label: &'static [u8],
    accent: Rgb,
}

const DESKTOP_ICONS: [DesktopIcon; 7] = [
    DesktopIcon { x: 24, y: ICONS_Y, label: b"Terminal", accent: Rgb(127, 168, 140) },
    DesktopIcon { x: 24 + (ICON_SIZE + ICON_GAP), y: ICONS_Y, label: b"Files", accent: Rgb(100, 150, 220) },
    DesktopIcon { x: 24 + (ICON_SIZE + ICON_GAP) * 2, y: ICONS_Y, label: b"Snake", accent: Rgb(80, 200, 120) },
    DesktopIcon { x: 24 + (ICON_SIZE + ICON_GAP) * 3, y: ICONS_Y, label: b"Paint", accent: Rgb(200, 100, 120) },
    DesktopIcon { x: 24 + (ICON_SIZE + ICON_GAP) * 4, y: ICONS_Y, label: b"Settings", accent: Rgb(140, 120, 200) },
    DesktopIcon { x: 24 + (ICON_SIZE + ICON_GAP) * 5, y: ICONS_Y, label: b"Apps", accent: Rgb(120, 180, 200) },
    DesktopIcon { x: 24 + (ICON_SIZE + ICON_GAP) * 6, y: ICONS_Y, label: b"Reboot", accent: Rgb(200, 120, 100) },
];

// ── Drawing helpers ──

/// Нарисовать закруглённый прямоугольник (скругление radius).
unsafe fn fill_round_rect(x: u32, y: u32, w: u32, h: u32, radius: u32, color: Rgb) {
    let r = radius.min(w / 2).min(h / 2);
    if r == 0 {
        framebuffer::fill_rect(x, y, w, h, color);
        return;
    }
    // center rect
    framebuffer::fill_rect(x + r, y, w - r * 2, r, color);
    framebuffer::fill_rect(x, y + r, w, h - r * 2, color);
    // corners
    for dy in 0..r {
        for dx in 0..r {
            if (dx as i32 - r as i32) * (dx as i32 - r as i32) + (dy as i32 - r as i32) * (dy as i32 - r as i32) <= (r as i32) * (r as i32) {
                framebuffer::put(x + r - 1 - dx, y + r - 1 - dy, color);
                framebuffer::put(x + w - r + dx, y + r - 1 - dy, color);
                framebuffer::put(x + r - 1 - dx, y + h - r + dy, color);
                framebuffer::put(x + w - r + dx, y + h - r + dy, color);
            }
        }
    }
}

/// Нарисовать мягкую тень (светлую полосу под элементом).
unsafe fn draw_soft_shadow(x: u32, y: u32, w: u32, h: u32, radius: u32) {
    let shadow = SHADOW;
    for dy in 1..=4 {
        let alpha = (4 - dy) as u8;
        for dx in -(radius as i32)..=(w as i32 + radius as i32) {
            let px = x as i32 + dx;
            let py = (y + h - 1 + dy) as i32;
            if px >= 0 && px < framebuffer::width() as i32 && py >= 0 && py < framebuffer::height() as i32 {
                framebuffer::put(px as u32, py as u32, Rgb(
                    shadow.0.saturating_sub(alpha * 10),
                    shadow.1.saturating_sub(alpha * 10),
                    shadow.2.saturating_sub(alpha * 10),
                ));
            }
        }
    }
}

unsafe fn save_menu_bg(mx: i32, my: i32) {
    let n = CTX_MENU_ITEMS.len() as u32;
    let mh = n * MENU_ITEM_H + MENU_PAD * 2;
    MENU_SAVED_X = mx.max(0).min(framebuffer::width() as i32 - MENU_W as i32);
    MENU_SAVED_Y = my.max(0).min(framebuffer::height() as i32 - mh as i32);
    MENU_SAVED_W = MENU_W.min(MENU_BG_W as u32);
    MENU_SAVED_H = mh.min(MENU_BG_H as u32);
    for row in 0..MENU_SAVED_H as usize {
        for col in 0..MENU_SAVED_W as usize {
            let px = (MENU_SAVED_X + col as i32) as u32;
            let py = (MENU_SAVED_Y + row as i32) as u32;
            if let Some(Rgb(r, g, b)) = framebuffer::get(px, py) {
                MENU_BG[row * MENU_BG_W + col] = (r as u32) | ((g as u32) << 8) | ((b as u32) << 16);
            }
        }
    }
}

unsafe fn restore_menu_bg() {
    for row in 0..MENU_SAVED_H as usize {
        for col in 0..MENU_SAVED_W as usize {
            let packed = MENU_BG[row * MENU_BG_W + col];
            let r = (packed & 0xFF) as u8;
            let g = ((packed >> 8) & 0xFF) as u8;
            let b = ((packed >> 16) & 0xFF) as u8;
            framebuffer::put(
                (MENU_SAVED_X + col as i32) as u32,
                (MENU_SAVED_Y + row as i32) as u32,
                Rgb(r, g, b),
            );
        }
    }
    MENU_ACTIVE = false;
}

/// Нарисовать контекстное меню в позиции (mx, my).
unsafe fn draw_context_menu(mx: i32, my: i32) {
    let n = CTX_MENU_ITEMS.len() as u32;
    let mh = n * MENU_ITEM_H + MENU_PAD * 2;

    let fb_w = framebuffer::width() as i32;
    let fb_h = framebuffer::height() as i32;
    let menu_x = mx.max(0).min(fb_w - MENU_W as i32);
    let menu_y = my.max(0).min(fb_h - mh as i32);
    if menu_x < 0 || menu_y < 0 { return; }

    save_menu_bg(mx, my);

    // Тень
    draw_soft_shadow(menu_x as u32, menu_y as u32, MENU_W, mh, MENU_RADIUS);

    // Фон меню (скруглённый, поверхность)
    fill_round_rect(menu_x as u32, menu_y as u32, MENU_W, mh, MENU_RADIUS, SURFACE);
    // Рамка
    let r = MENU_RADIUS.min(MENU_W / 2).min(mh / 2);
    graphics::draw_rect(menu_x as u32 + r, menu_y as u32, MENU_W - r * 2, mh, BORDER.0, BORDER.1, BORDER.2, false);

    // Пункты
    let mut iy = menu_y as u32 + MENU_PAD;
    for item in &CTX_MENU_ITEMS {
        if item.label == b"---------" {
            let sep_y = iy + MENU_ITEM_H / 2;
            for sx in (menu_x as u32 + 8)..(menu_x as u32 + MENU_W - 8) {
                framebuffer::put(sx, sep_y, BORDER);
            }
        } else {
            let mut lx = menu_x as u32 + 12;
            for &ch in item.label {
                framebuffer::draw_char_boot(lx, iy + 5, ch, TEXT_FG, 1);
                lx += 8;
            }
        }
        iy += MENU_ITEM_H;
    }

    MENU_ACTIVE = true;
}

/// Обработать клик по пункту контекстного меню.
/// Возвращает true, если клик был по меню (и действие выполнено).
unsafe fn handle_menu_click(mx: i32, my: i32) -> bool {
    if !MENU_ACTIVE { return false; }

    let n = CTX_MENU_ITEMS.len() as u32;
    let mh = n * MENU_ITEM_H + MENU_PAD * 2;

    if mx < MENU_SAVED_X || mx >= MENU_SAVED_X + MENU_W as i32
        || my < MENU_SAVED_Y || my >= MENU_SAVED_Y + mh as i32
    {
        restore_menu_bg();
        return false;
    }

    let rel_y = (my - MENU_SAVED_Y) as u32;
    let idx = rel_y.saturating_sub(MENU_PAD) / MENU_ITEM_H;
    if (idx as usize) >= CTX_MENU_ITEMS.len() {
        restore_menu_bg();
        return false;
    }

    let action = CTX_MENU_ITEMS[idx as usize].action;
    restore_menu_bg();

    let fb_w = framebuffer::width();
    let fb_h = framebuffer::height();
    let win_w = 320u32.min(fb_w - 40);
    let win_h = 240u32.min(fb_h - 100);
    let win_x = (fb_w - win_w) / 2;
    let win_y = (fb_h - win_h) / 3;

    match action {
        MenuAction::Terminal => {
            let id = window::create(b"Terminal", win_x, win_y, win_w, win_h);
            if id >= 0 {
                window::set_app_kind(id as u32, window::AppKind::Terminal);
                window::terminal_write_str(b"PureOS Terminal v0.4\n> ");
            }
            return false;
        }
        MenuAction::Files => {
            let id = window::create(b"Files", win_x, win_y, win_w, win_h);
            if id >= 0 {
                window::set_app_kind(id as u32, window::AppKind::Files);
            }
            return false;
        }
        MenuAction::Snake => {
            let id = window::create(b"Snake", win_x, win_y, 280, 220);
            if id >= 0 {
                window::set_app_kind(id as u32, window::AppKind::Snake);
            }
            return false;
        }
        MenuAction::Apps => {
            let id = window::create(b"Apps", win_x, win_y, win_w, win_h);
            if id >= 0 {
                window::set_app_kind(id as u32, window::AppKind::Desktop);
            }
            return false;
        }
        MenuAction::Settings => {
            let id = window::create(b"Settings", win_x, win_y, win_w, win_h);
            if id >= 0 {
                window::set_app_kind(id as u32, window::AppKind::Settings);
            }
            return false;
        }
        MenuAction::Paint => {
            let id = window::create(b"Paint", win_x, win_y, 300, 260);
            if id >= 0 {
                window::set_app_kind(id as u32, window::AppKind::Paint);
            }
            return false;
        }
        MenuAction::Wallpaper => {
            crate::wallpaper::set_wallpaper_by_name(b"bg1");
            draw_desktop_icons();
            draw_dock();
            return false;
        }
        MenuAction::Reboot => {
            clear_desktop();
            terminal::write(b"\n[Desktop] Rebooting...\n");
            crate::uefi::reset_system();
            loop { core::hint::spin_loop(); }
        }
        MenuAction::Shutdown => {
            clear_desktop();
            terminal::write(b"\n[Desktop] Shutting down...\n");
            crate::uefi::shutdown();
            loop { core::hint::spin_loop(); }
        }
        MenuAction::Properties => {
            let (mx2, my2) = crate::usb::mouse_pos();
            let mut found = false;
            for icon in &DESKTOP_ICONS {
                if (mx2 as u32) >= icon.x && (mx2 as u32) < icon.x + ICON_SIZE
                    && (my2 as u32) >= icon.y && (my2 as u32) < icon.y + ICON_SIZE
                {
                    terminal::write(b"\n[Desktop] Icon: ");
                    terminal::write(icon.label);
                    terminal::write(b"\n  Position: (");
                    terminal::write_num(icon.x as u64);
                    terminal::write(b", ");
                    terminal::write_num(icon.y as u64);
                    terminal::write(b")\n  Size: ");
                    terminal::write_num(ICON_SIZE as u64);
                    terminal::write(b"x");
                    terminal::write_num(ICON_SIZE as u64);
                    terminal::write(b"\n");
                    found = true;
                    break;
                }
            }
            if !found {
                terminal::write(b"\nNo icon at this position.\n");
            }
            crate::wallpaper::set_wallpaper_by_name(b"bg1");
            draw_desktop_icons();
            draw_dock();
            return false;
        }
        MenuAction::Cancel => {
            return false; // just dismiss
        }
    }
}

/// Главная функция десктопа. Вызывается из shell командой `desktop`.
/// Рисует рабочий стол и входит в цикл обработки событий.
/// Возврат при нажатии Escape.
pub unsafe fn run() {
    if framebuffer::width() == 0 || framebuffer::height() == 0 {
        terminal::write(b"desktop: no framebuffer\n");
        return;
    }

    // 0. Инициализировать оконный менеджер
    window::init();

    // 1. Нарисовать фон (обои) — bg1 по умолчанию, fallback на waves
    if !crate::wallpaper::set_wallpaper_by_name(b"bg1") {
        crate::wallpaper::set_wallpaper_by_name(b"waves");
    }

    // 2. Нарисовать иконки рабочего стола
    draw_desktop_icons();

    // 3. Нарисовать док (панель снизу по центру)
    draw_dock();

    // 4. Инициализировать и показать курсор
    crate::usb::mouse_init();
    crate::usb::mouse_show();

    // 5. Главный цикл десктопа
    desktop_loop();
}

unsafe fn desktop_loop() {
    let mut prev_buttons: u8 = 0;
    let mut drag_win: u32 = 0;
    let mut resize_win: u32 = 0;
    let mut mouse_pressed: bool = false;
    let mut right_pressed: bool = false;

    loop {
        // USB poll (keyboard + mouse)
        crate::usb::poll();

        // PS/2 мышь/тачпад
        crate::ps2mouse::poll();

        // Обновить курсор мыши
        crate::usb::mouse_poll();

        let (mx, my) = crate::usb::mouse_pos();
        let buttons = crate::usb::mouse_buttons();
        let left_down = buttons & 0x01 != 0;
        let right_down = buttons & 0x02 != 0;
        let left_pressed = left_down && !mouse_pressed;
        let left_released = !left_down && mouse_pressed;
        let right_clicked = right_down && !right_pressed;

        // Чтение клавиатуры
        let mut got_escape = false;
        while let Some(ch) = crate::usb::key_read() {
            match ch {
                0x1B => got_escape = true,
                keyboard::KEY_META => {
                    if START_MENU_ACTIVE { close_start_menu(); }
                    else {
                        if MENU_ACTIVE { restore_menu_bg(); draw_desktop_icons(); }
                        draw_start_menu();
                    }
                }
                b'\n' | b'\r' => {
                    if START_MENU_ACTIVE {
                        let action = START_ITEMS[START_SEL].action;
                        exec_start_action(action);
                        break;
                    }
                }
                b'w' | b'W' | 0x1E => {
                    if START_MENU_ACTIVE && START_SEL > 0 {
                        START_SEL -= 1;
                        close_start_menu();
                        draw_start_menu();
                    }
                }
                b's' | b'S' | 0x1F => {
                    if START_MENU_ACTIVE && START_SEL + 1 < START_ITEMS.len() {
                        START_SEL += 1;
                        close_start_menu();
                        draw_start_menu();
                    }
                }
                _ => {
                    // Send to active terminal window
                    window::send_key_to_active(ch);
                }
            }
        }
        keyboard::poll();
        while let Some(ch) = keyboard::read_key() {
            match ch {
                0x1B => got_escape = true,
                keyboard::KEY_META => {
                    if START_MENU_ACTIVE { close_start_menu(); }
                    else {
                        if MENU_ACTIVE { restore_menu_bg(); draw_desktop_icons(); }
                        draw_start_menu();
                    }
                }
                b' ' | b'\n' | b'\r' => {
                    if !MENU_ACTIVE && !START_MENU_ACTIVE && window::open_count() == 0 {
                        let (mx, my) = crate::usb::mouse_pos();
                        handle_icon_click(mx as u32, my as u32);
                    }
                }
                _ => {
                    window::send_key_to_active(ch);
                }
            }
        }
        if got_escape {
            if START_MENU_ACTIVE { close_start_menu(); }
            else if MENU_ACTIVE { restore_menu_bg(); draw_desktop_icons(); }
            else if window::open_count() > 0 {
                // Close all windows
                for i in 0..8 {
                    if let Some(win) = window::get(i as u32) {
                        window::close(win.id);
                    }
                }
            } else {
                clear_desktop();
                return;
            }
        }

        // ═══ Mouse events ═══
        if left_pressed {
            let fb_h = framebuffer::height();
            let sep_y = fb_h - DOCK_H;

            // Проверка клика по кнопке PureOS (Start menu)
            if my >= sep_y as i32 + 2 && my < fb_h as i32 - 2 && mx >= 2 && mx < 74 {
                if START_MENU_ACTIVE { close_start_menu(); }
                else {
                    if MENU_ACTIVE { restore_menu_bg(); draw_desktop_icons(); }
                    draw_start_menu();
                }
                mouse_pressed = left_down;
                continue;
            }

            if START_MENU_ACTIVE {
                let sm_y = sep_y - (START_ITEMS.len() as u32 * START_MENUITEM_H + 8);
                if mx >= 2 && mx < 2 + START_MENU_W as i32
                    && my >= sm_y as i32 && my < sep_y as i32
                {
                    let rel_y = my - sm_y as i32;
                    let idx = (rel_y as u32 - 24) / START_MENUITEM_H;
                    if (idx as usize) < START_ITEMS.len() {
                        START_SEL = idx as usize;
                        let action = START_ITEMS[START_SEL].action;
                        exec_start_action(action);
                        mouse_pressed = left_down;
                        continue;
                    }
                }
                close_start_menu();
                mouse_pressed = left_down;
                continue;
            }

            if MENU_ACTIVE {
                if handle_menu_click(mx, my) {
                    mouse_pressed = left_down;
                    continue;
                }
            }

            // Check click on taskbar window indicators
            let n_win = window::open_count();
            if n_win > 0 {
                let fb_w = framebuffer::width();
                let dock_w2 = (DESKTOP_ICONS.len() as u32 * (ICON_SIZE + ICON_GAP)).min(fb_w - 40);
                let dock_x2 = (fb_w - dock_w2) / 2;
                let tx2 = dock_x2 + dock_w2 + 4;
                let ty2 = fb_h - DOCK_H - 8 + 6;
                let w_limit = (n_win as u32 * 24).min(fb_w - dock_x2 - dock_w2 - 100);
                for wi in 0..n_win {
                    let ix = tx2 + wi as u32 * 24;
                    if mx >= ix as i32 && mx < (ix + 20) as i32
                        && my >= ty2 as i32 && my < (ty2 + DOCK_H - 12) as i32
                    {
                        if let Some(wid) = window::window_id_at(wi) {
                            if window::is_minimized(wid) {
                                window::restore(wid);
                            } else {
                                window::bring_to_front(wid);
                            }
                            draw_dock();
                            window::redraw_all();
                        }
                        mouse_pressed = left_down;
                        continue;
                    }
                }
            }

            // Try window click first
            let (win_id, action) = window::handle_mouse(mx, my, true);
            if action == 1 {
                drag_win = win_id;
            } else if action == 2 {
                window::close(win_id);
                // Redraw desktop background
                draw_desktop_icons();
                draw_dock();
                window::redraw_all();
            } else if action == 4 {
                window::minimize(win_id);
                draw_dock();
                window::redraw_all();
            } else if action == 5 {
                resize_win = win_id;
            } else if action == 0 && win_id == 0 {
                // Outside any window → handle icon click
                handle_icon_click(mx as u32, my as u32);
            }
        }

        // Right click → context menu (only on desktop, not on windows)
        if right_clicked {
            if !MENU_ACTIVE && !START_MENU_ACTIVE && window::open_count() == 0 {
                draw_context_menu(mx, my);
            }
        }

        // Handle dragging
        if left_down && drag_win != 0 {
            window::drag_update(drag_win, mx, my);
        }
        // Handle resizing
        if left_down && resize_win != 0 {
            window::resize_update(resize_win, mx, my);
        }
        if left_released {
            if drag_win != 0 || resize_win != 0 {
                window::drag_end();
                draw_dock();
            }
            drag_win = 0;
            resize_win = 0;
        }

        prev_buttons = buttons;
        mouse_pressed = left_down;
        right_pressed = right_down;

        // Render windows
        window::redraw_all();
        window::render_all_content();

        // Обновить часы каждые ~100 циклов
        update_clock();

        // Небольшая пауза
        for _ in 0..10000 { core::hint::spin_loop(); }
    }
}

/// Нарисовать иконку рабочего стола (скруглённый квадрат с акцентным цветом).
unsafe fn draw_desktop_icon(x: u32, y: u32, label: &[u8], accent: Rgb) {
    // Фон (скруглённый квадрат с цветом акцента)
    fill_round_rect(x, y, ICON_SIZE, ICON_SIZE, 12, accent);
    let r = 12u32.min(ICON_SIZE / 2);
    graphics::draw_rect(x + r, y, ICON_SIZE - r * 2, ICON_SIZE, BORDER.0, BORDER.1, BORDER.2, false);

    // Нарисовать .bin иконку поверх фона
    let data = icon_data_for(label);
    draw_raw_icon(x, y, data);

    // Если .bin нет или он пустой — глиф-фолбэк
    if data.len() < 4 {
        let glyph: u8 = match label {
            b"Terminal" => b'>',
            b"Files"    => b'F',
            b"Snake"    => b'S',
            b"Paint"    => b'P',
            b"Settings" => b'*',
            b"Reboot"   => b'R',
            _           => b'?',
        };
        framebuffer::draw_char_boot(x + ICON_SIZE / 2 - 4, y + ICON_SIZE / 2 - 8, glyph, Rgb(255, 255, 255), 1);
    }
}

unsafe fn draw_desktop_icons() {
    for icon in &DESKTOP_ICONS {
        draw_desktop_icon(icon.x, icon.y, icon.label, icon.accent);

        // Подложка под label (полупрозрачный тёмный фон для читаемости)
        let label_y = icon.y + ICON_SIZE + 2;
        let fw = 8u32;
        let label_w = icon.label.len() as u32 * fw + 4;
        let lx = icon.x + (ICON_SIZE + 4).saturating_sub(label_w) / 2;
        // Dark semi-transparent background
        for by in 0..12 {
            for bx in 0..label_w {
                let alpha = if by < 2 || by >= 10 { 60 } else { 100 };
                let px = lx + bx;
                let py = label_y + by;
                if px < framebuffer::width() && py < framebuffer::height() {
                    if let Some(bg) = framebuffer::get(px, py) {
                        framebuffer::put(px, py, framebuffer::alpha_blend(bg, Rgb(0, 0, 0), alpha));
                    }
                }
            }
        }
        let mut tx = lx + 2;
        for &ch in icon.label {
            framebuffer::draw_char_boot(tx, label_y + 2, ch, Rgb(240, 245, 240), 1);
            tx += fw;
        }
    }
}

unsafe fn draw_dock() {
    let fb_w = framebuffer::width();
    let fb_h = framebuffer::height();
    if fb_h <= DOCK_H || fb_w == 0 { return; }

    // Док — по центру снизу, скруглённый
    let dock_w = (DESKTOP_ICONS.len() as u32 * (ICON_SIZE + ICON_GAP)).min(fb_w - 40);
    let dock_x = (fb_w - dock_w) / 2;
    let dock_y = fb_h - DOCK_H - 8;

    // Тень под доком
    draw_soft_shadow(dock_x, dock_y, dock_w, DOCK_H, DOCK_RADIUS);

    // Фон дока (полупрозрачный SURFACE)
    fill_round_rect(dock_x, dock_y, dock_w, DOCK_H, DOCK_RADIUS, SURFACE);
    // Рамка
    let r = DOCK_RADIUS.min(dock_w / 2).min(DOCK_H / 2);
    graphics::draw_rect(dock_x + r, dock_y, dock_w - r * 2, DOCK_H, BORDER.0, BORDER.1, BORDER.2, false);

    // Иконки в доке
    let icon_start_x = dock_x + (dock_w - (DESKTOP_ICONS.len() as u32 * (ICON_SIZE + 8) - 8)) / 2;
    for (i, icon) in DESKTOP_ICONS.iter().enumerate() {
        let ix = icon_start_x + i as u32 * (ICON_SIZE + 8);
        let iy = dock_y + (DOCK_H - ICON_SIZE) / 2;
        draw_desktop_icon(ix, iy, icon.label, icon.accent);
    }

    // Таскбар: открытые окна — индикаторы справа от иконок
    let n_win = window::open_count();
    if n_win > 0 {
        let tw = (n_win as u32 * 24).min(fb_w - dock_x - dock_w - 100);
        let tx = dock_x + dock_w + 4;
        let ty = dock_y + 6;
        let mut win_idx = 0;
        for _ in 0..8 {
            if let Some(wid) = window::window_id_at(win_idx) {
                let is_min = window::is_minimized(wid);
                let ix = tx + win_idx as u32 * 24;
                let color = if is_min { Rgb(160, 170, 160) } else { Rgb(100, 160, 130) };
                framebuffer::fill_rect(ix, ty, 20, DOCK_H - 12, color);
                graphics::draw_rect(ix, ty, 20, DOCK_H - 12, Rgb(180, 195, 185).0, Rgb(180, 195, 185).1, Rgb(180, 195, 185).2, false);
                // Tiny icon dot
                framebuffer::fill_rect(ix + 6, ty + (DOCK_H - 12) / 2 - 2, 8, 4, if is_min { Rgb(200, 200, 200) } else { Rgb(230, 255, 240) });
            }
            win_idx += 1;
        }
    }

    // Часы — справа от дока
    let clock_w = 80u32;
    let clock_x = (dock_x + dock_w + 8 + if n_win > 0 { (n_win as u32 * 24).min(fb_w - dock_x - dock_w - 100) + 4 } else { 0 }).min(fb_w - clock_w);
    framebuffer::fill_rect(clock_x, dock_y + 4, clock_w, DOCK_H - 8, SURFACE);
    fill_round_rect(clock_x, dock_y + 4, clock_w, DOCK_H - 8, 12, SURFACE);
}

/// Нарисовать меню «Пуск» (Start menu) — скруглённое, над доком.
unsafe fn draw_start_menu() {
    let fb_h = framebuffer::height();
    let fb_w = framebuffer::width();
    let sep_y = fb_h - DOCK_H - 8;
    let sm_w = START_MENU_W;
    let sm_h = START_ITEMS.len() as u32 * START_MENUITEM_H + 36;
    let sm_x = (fb_w - sm_w) / 2;
    let sm_y = sep_y - sm_h;

    // Тень
    draw_soft_shadow(sm_x, sm_y, sm_w, sm_h, START_RADIUS);

    // Фон
    fill_round_rect(sm_x, sm_y, sm_w, sm_h, START_RADIUS, SURFACE);
    let r = START_RADIUS;
    graphics::draw_rect(sm_x + r, sm_y, sm_w - r * 2, sm_h, BORDER.0, BORDER.1, BORDER.2, false);

    // Заголовок
    let header = b"  PureOS";
    let mut hx = sm_x + 12;
    for &ch in header {
        framebuffer::draw_char_boot(hx, sm_y + 8, ch, DARK_ACCENT, 1);
        hx += 8;
    }
    // Разделитель
    for dx in sm_x + 10..sm_x + sm_w - 10 {
        framebuffer::put(dx, sm_y + 28, BORDER);
    }

    // Пункты меню
    for (i, item) in START_ITEMS.iter().enumerate() {
        let iy = sm_y + 32 + i as u32 * START_MENUITEM_H;
        let bg = if i == START_SEL { ACCENT } else { SURFACE };
        let fg = if i == START_SEL { Rgb(255, 255, 255) } else { TEXT_FG };
        if i == START_SEL {
            fill_round_rect(sm_x + 4, iy + 2, sm_w - 8, START_MENUITEM_H - 4, BTN_RADIUS, bg);
        }
        let mut lx = sm_x + 16;
        for &ch in item.label {
            framebuffer::draw_char_boot(lx, iy + 6, ch, fg, 1);
            lx += 8;
        }
    }

    START_MENU_ACTIVE = true;
}

/// Закрыть меню «Пуск» и восстановить док.
unsafe fn close_start_menu() {
    START_MENU_ACTIVE = false;
    crate::wallpaper::set_wallpaper_by_name(b"bg1");
    draw_desktop_icons();
    draw_dock();
}

/// Выполнить действие из стартового меню.
unsafe fn exec_start_action(action: MenuAction) {
    close_start_menu();
    let fb_w = framebuffer::width();
    let fb_h = framebuffer::height();
    let win_w = 320u32.min(fb_w - 40);
    let win_h = 240u32.min(fb_h - 100);
    let win_x = (fb_w - win_w) / 2;
    let win_y = (fb_h - win_h) / 3;

    match action {
        MenuAction::Terminal => {
            let id = window::create(b"Terminal", win_x, win_y, win_w, win_h);
            if id >= 0 {
                window::set_app_kind(id as u32, window::AppKind::Terminal);
                window::terminal_write_str(b"PureOS Terminal v0.4\n> ");
            }
        }
        MenuAction::Files => {
            let id = window::create(b"Files", win_x, win_y, win_w, win_h);
            if id >= 0 {
                window::set_app_kind(id as u32, window::AppKind::Files);
            }
        }
        MenuAction::Snake => {
            let id = window::create(b"Snake", win_x, win_y, 280, 220);
            if id >= 0 {
                window::set_app_kind(id as u32, window::AppKind::Snake);
            }
        }
        MenuAction::Paint => {
            let id = window::create(b"Paint", win_x, win_y, 300, 260);
            if id >= 0 {
                window::set_app_kind(id as u32, window::AppKind::Paint);
            }
        }
        MenuAction::Settings => {
            let id = window::create(b"Settings", win_x, win_y, win_w, win_h);
            if id >= 0 {
                window::set_app_kind(id as u32, window::AppKind::Settings);
            }
        }
        MenuAction::Wallpaper => {
            crate::wallpaper::set_wallpaper_by_name(b"bg1");
            draw_desktop_icons();
            draw_dock();
        }
        MenuAction::Reboot => {
            clear_desktop();
            terminal::write(b"\n[Desktop] Rebooting...\n");
            crate::uefi::reset_system();
            loop { core::hint::spin_loop(); }
        }
        _ => {}
    }
}

unsafe fn update_clock() {
    CLOCK_TICK = CLOCK_TICK.wrapping_add(1);
    if CLOCK_TICK % 50 != 0 { return; }

    if !CLOCK_INIT {
        CLOCK_INIT = true;
        // Попробовать прочитать из RTC
        if !CLOCK_RTC_INIT {
            CLOCK_RTC_INIT = true;
            let (h, m) = crate::cmos::read_time();
            CLOCK_HOUR = h as u32;
            CLOCK_MIN = m as u32;
        } else {
            CLOCK_HOUR = 12;
            CLOCK_MIN = 0;
        }
    }

    // Раз в 60 тиков (~1 сек при 50-тик интервале) обновлять из RTC
    if CLOCK_TICK % 3000 == 0 && CLOCK_RTC_INIT {
        let (h, m) = crate::cmos::read_time();
        CLOCK_HOUR = h as u32;
        CLOCK_MIN = m as u32;
    } else {
        CLOCK_MIN = CLOCK_MIN.wrapping_add(1);
        if CLOCK_MIN >= 60 {
            CLOCK_MIN = 0;
            CLOCK_HOUR = (CLOCK_HOUR + 1) % 24;
        }
    }

    let fb_w = framebuffer::width();
    let fb_h = framebuffer::height();
    let dock_w = (DESKTOP_ICONS.len() as u32 * (ICON_SIZE + ICON_GAP)).min(fb_w - 40);
    let dock_x = (fb_w - dock_w) / 2;
    let dock_y = fb_h - DOCK_H - 8;
    let clock_x = (dock_x + dock_w + 8).min(fb_w - 80);
    let clock_w = 80u32;

    // Стереть старые цифры
    framebuffer::fill_rect(clock_x, dock_y + 4, clock_w, DOCK_H - 8, SURFACE);
    fill_round_rect(clock_x, dock_y + 4, clock_w, DOCK_H - 8, 12, SURFACE);

    // Нарисовать часы
    let time_str = [
        b'0' + (CLOCK_HOUR / 10) as u8,
        b'0' + (CLOCK_HOUR % 10) as u8,
        b':',
        b'0' + (CLOCK_MIN / 10) as u8,
        b'0' + (CLOCK_MIN % 10) as u8,
    ];
    let mut tx = clock_x + 8;
    for &ch in &time_str {
        framebuffer::draw_char_boot(tx, dock_y + 22, ch, TEXT_FG, 1);
        tx += 8;
    }
}

unsafe fn handle_icon_click(mx: u32, my: u32) {
    for icon in &DESKTOP_ICONS {
        if mx >= icon.x && mx < icon.x + ICON_SIZE
            && my >= icon.y && my < icon.y + ICON_SIZE
        {
            // Подсветка иконки (чуть светлее)
            let h = icon.accent;
            let highlight = Rgb(
                h.0.saturating_add(30).min(255),
                h.1.saturating_add(30).min(255),
                h.2.saturating_add(30).min(255),
            );
            fill_round_rect(icon.x, icon.y, ICON_SIZE, ICON_SIZE, 12, highlight);

            let fb_w = framebuffer::width();
            let fb_h = framebuffer::height();
            let win_w = 320u32.min(fb_w - 40);
            let win_h = 240u32.min(fb_h - 100);
            let win_x = (fb_w - win_w) / 2;
            let win_y = (fb_h - win_h) / 3;

            match icon.label {
                b"Terminal" => {
                    let id = window::create(b"Terminal", win_x, win_y, win_w, win_h);
                    if id >= 0 {
                        window::set_app_kind(id as u32, window::AppKind::Terminal);
                        window::terminal_write_str(b"PureOS Terminal v0.4\n");
                        window::terminal_write_str(b"Type here...\n> ");
                    }
                    return;
                }
                b"Files" => {
                    let id = window::create(b"Files", win_x, win_y, win_w, win_h);
                    if id >= 0 {
                        window::set_app_kind(id as u32, window::AppKind::Files);
                    }
                    return;
                }
                b"Snake" => {
                    let id = window::create(b"Snake", win_x, win_y, 280, 220);
                    if id >= 0 {
                        window::set_app_kind(id as u32, window::AppKind::Snake);
                    }
                    return;
                }
                b"Paint" => {
                    let id = window::create(b"Paint", win_x, win_y, 300, 260);
                    if id >= 0 {
                        window::set_app_kind(id as u32, window::AppKind::Paint);
                    }
                    return;
                }
                b"Settings" => {
                    let id = window::create(b"Settings", win_x, win_y, win_w, win_h);
                    if id >= 0 {
                        window::set_app_kind(id as u32, window::AppKind::Settings);
                    }
                    return;
                }
                b"Apps" => {
                    let id = window::create(b"Apps", win_x, win_y, win_w, win_h);
                    if id >= 0 {
                        window::set_app_kind(id as u32, window::AppKind::Desktop);
                        window::terminal_write_str(b"Launch apps from shell:\n");
                        window::terminal_write_str(b"  exec <name>\n");
                    }
                    return;
                }
                b"Reboot" => {
                    clear_desktop();
                    terminal::write(b"\n[Desktop] Rebooting...\n");
                    crate::uefi::reset_system();
                    loop { core::hint::spin_loop(); }
                }
                _ => {}
            }

            // Вернуть оригинальный вид
            draw_desktop_icon(icon.x, icon.y, icon.label, icon.accent);
            break;
        }
    }
}

/// Сканировать /apps/ на наличие .pos-файлов и предложить запуск.
unsafe fn list_and_run_pos_files() {
    terminal::write(b"\n[Desktop] Scanning /apps/ for .pos files...\n");

    // Убедиться, что каталог /apps/ существует
    let apps_dir = crate::fs::resolve(b"/apps").unwrap_or(crate::fs::ROOT);

    // Собрать список .pos-файлов
    let mut entries: [(u16, [u8; 32]); 16] = [(0, [0; 32]); 16];
    let mut count = 0usize;

    crate::fs::for_each_child(apps_dir, |idx| {
        if count >= entries.len() { return; }
        if crate::fs::kind(idx) != crate::fs::Kind::File { return; }
        let name = crate::fs::node_name(idx);
        if name.len() < 5 { return; }
        if !name[name.len()-4..].eq_ignore_ascii_case(b".pos") { return; }

        let mut buf = [0u8; 32];
        let len = name.len().min(31);
        buf[..len].copy_from_slice(&name[..len]);
        buf[len] = 0;
        entries[count] = (idx, buf);
        count += 1;
    });

    if count == 0 {
        terminal::write(b"No .pos files found in /apps/.\n");
        terminal::write(b"Copy .pos files to /apps/ using:\n");
        terminal::write(b"  cp <source> /apps/\n");
        terminal::write(b"\nType 'desktop' to return.\n");
        return;
    }

    // Показать список
    for i in 0..count {
        terminal::write(b"  [");
        terminal::write_num(i as u64);
        terminal::write(b"] ");
        let mut pos = 0;
        while entries[i].1[pos] != 0 {
            let ch = entries[i].1[pos];
            let s = [ch];
            terminal::write(&s);
            pos += 1;
        }
        terminal::write(b"\n");
    }

    terminal::write(b"\nEnter number to run (or empty to cancel): ");
    let mut buf = [0u8; 8];
    let mut len = 0usize;
    loop {
        crate::usb::poll();
        if let Some(ch) = crate::usb::key_read() {
            match ch {
                0x0D | 0x0A => {
                    terminal::write(b"\n");
                    break;
                }
                0x08 => {
                    if len > 0 {
                        len -= 1;
                        terminal::write(b"\x08 \x08");
                    }
                }
                0x1B => { terminal::write(b"\n"); return; }
                ch if ch >= b'0' && ch <= b'9' && len < buf.len() => {
                    buf[len] = ch;
                    len += 1;
                    let s = [ch];
                    terminal::write(&s);
                }
                _ => {}
            }
        }
        keyboard::poll();
        while let Some(ch) = keyboard::read_key() {
            if ch == 0x1B { terminal::write(b"\n"); return; }
        }
    }

    if len == 0 { return; }
    let mut choice: usize = 0;
    for i in 0..len {
        choice = choice * 10 + (buf[i] - b'0') as usize;
    }
    if choice >= count {
        terminal::write(b"Invalid selection.\n");
        return;
    }

    let (idx, name_buf) = entries[choice];
    let name = &name_buf[..name_buf.iter().position(|&c| c == 0).unwrap_or(32)];

    terminal::write(b"Running ");
    terminal::write(name);
    terminal::write(b"...\n");

    let data = crate::fs::read(idx);
    if data.len() < 16 {
        terminal::write(b"File too small.\n");
        return;
    }

    let pid = crate::pos::exec(data);
    if pid < 0 {
        terminal::write(b"Error: ");
        terminal::write_num((-pid) as u64);
        terminal::write(b"\n");
    } else {
        terminal::write(b"Process ");
        terminal::write_num(pid as u64);
        terminal::write(b" started.\n");
    }
}

/// Восстановить текстовый терминал при выходе из десктопа.
unsafe fn clear_desktop() {
    crate::usb::mouse_show();
    terminal::clear();
    terminal::write(b"Desktop closed. Back to shell.\n");
}
