//! Оконная система PureOS — лёгкий менеджер окон.
//!
//! Zero-Alloc: фиксированный массив окон (MAX_WINDOWS = 8),
//! кадр сохраняется/восстанавливается под окнами.
//! Поддерживает: заголовок, кнопка закрытия, перетаскивание,
//! z-order, backdrop-blur (glassmorphism).

use crate::framebuffer::{self, Rgb};
use crate::graphics;

/// Максимум окон.
const MAX_WINDOWS: usize = 8;

/// Радиус скругления углов окна.
const WIN_RADIUS: u32 = 14;

/// Высота заголовка окна.
const TITLE_H: u32 = 28;

/// Отступ содержимого от рамки.
const CONTENT_PAD: u32 = 2;

/// Цвета окна.
const TITLE_BG: Rgb = Rgb(55, 65, 58);
const TITLE_FG: Rgb = Rgb(230, 240, 230);
const CLOSE_BTN: Rgb = Rgb(220, 80, 80);
const CLOSE_BTN_HOVER: Rgb = Rgb(240, 60, 60);
const MINIMIZE_BTN: Rgb = Rgb(180, 170, 60);
const MINIMIZE_BTN_HOVER: Rgb = Rgb(210, 200, 80);
const RESIZE_HANDLE_COLOR: Rgb = Rgb(150, 165, 155);
const WIN_BORDER: Rgb = Rgb(180, 195, 185);
const WIN_BG: Rgb = Rgb(30, 35, 32);
const SHADOW_COLOR: Rgb = Rgb(20, 25, 22);

/// Состояние окна.
#[derive(Clone, Copy, PartialEq)]
pub enum WinState {
    Closed,
    Open,
}

/// Окно десктопа.
#[derive(Clone, Copy)]
pub struct Window {
    pub id: u32,
    pub state: WinState,
    pub title: &'static [u8],
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub content_x: u32,
    pub content_y: u32,
    pub content_w: u32,
    pub content_h: u32,
    pub dragging: bool,
    pub drag_off_x: i32,
    pub drag_off_y: i32,
    pub close_hover: bool,
    pub min_hover: bool,
    pub minimized: bool,
    pub saved_x: u32,
    pub saved_y: u32,
    pub resizing: bool,
    pub resize_dir: u8, // 0=none, 1=right, 2=bottom, 3=corner
    pub z: u32,  // z-order (higher = on top)
}

/// Глобальный менеджер окон.
static mut WINDOWS: [Window; MAX_WINDOWS] = [Window {
    id: 0, state: WinState::Closed,
    title: b"", x: 0, y: 0, w: 0, h: 0,
    content_x: 0, content_y: 0, content_w: 0, content_h: 0,
    dragging: false, drag_off_x: 0, drag_off_y: 0,
    close_hover: false, min_hover: false,
    minimized: false, saved_x: 0, saved_y: 0,
    resizing: false, resize_dir: 0,
    z: 0,
}; MAX_WINDOWS];
static mut WINDOW_COUNT: usize = 0;
static mut NEXT_ID: u32 = 1;
static mut TOP_Z: u32 = 0;

// ── Background save bufs (one per window slot, up to 256x256 area) ──
const BG_BUF_SIZE: usize = 256 * 256;
static mut WIN_BG_BUF: [u32; BG_BUF_SIZE] = [0; BG_BUF_SIZE];

/// Инициализировать менеджер окон.
pub unsafe fn init() {
    WINDOW_COUNT = 0;
    NEXT_ID = 1;
    TOP_Z = 0;
    for w in WINDOWS.iter_mut() {
        w.state = WinState::Closed;
        w.dragging = false;
    }
}

/// Найти свободный слот окна.
fn find_slot() -> Option<usize> {
    for i in 0..MAX_WINDOWS {
        if unsafe { WINDOWS[i].state == WinState::Closed } {
            return Some(i);
        }
    }
    None
}

/// Найти окно по ID (включая минимизированные).
fn find_by_id(id: u32) -> Option<usize> {
    for i in 0..MAX_WINDOWS {
        if unsafe { WINDOWS[i].id == id && (WINDOWS[i].state == WinState::Open || WINDOWS[i].minimized) } {
            return Some(i);
        }
    }
    None
}

/// Найти окно по ID (только Open).
fn find_open_by_id(id: u32) -> Option<usize> {
    for i in 0..MAX_WINDOWS {
        if unsafe { WINDOWS[i].id == id && WINDOWS[i].state == WinState::Open } {
            return Some(i);
        }
    }
    None
}

/// Создать новое окно. Возвращает ID окна или -1.
pub unsafe fn create(title: &'static [u8], x: u32, y: u32, w: u32, h: u32) -> i32 {
    let slot = match find_slot() {
        Some(s) => s,
        None => return -1,
    };
    let id = NEXT_ID;
    NEXT_ID += 1;
    TOP_Z += 1;

    let content_w = if w > CONTENT_PAD * 2 { w - CONTENT_PAD * 2 } else { 1 };
    let content_h = if h > TITLE_H + CONTENT_PAD * 2 { h - TITLE_H - CONTENT_PAD * 2 } else { 1 };

    WINDOWS[slot] = Window {
        id,
        state: WinState::Open,
        title,
        x, y, w, h,
        content_x: x + CONTENT_PAD,
        content_y: y + TITLE_H + CONTENT_PAD,
        content_w,
        content_h,
        dragging: false,
        drag_off_x: 0,
        drag_off_y: 0,
        close_hover: false,
        min_hover: false,
        minimized: false,
        saved_x: x,
        saved_y: y,
        resizing: false,
        resize_dir: 0,
        z: TOP_Z,
    };
    id as i32
}

/// Закрыть окно по ID.
pub unsafe fn close(id: u32) {
    if let Some(slot) = find_by_id(id) {
        WINDOWS[slot].state = WinState::Closed;
    }
}

/// Установить z-order окна на верх.
pub unsafe fn bring_to_front(id: u32) {
    if let Some(slot) = find_by_id(id) {
        TOP_Z += 1;
        WINDOWS[slot].z = TOP_Z;
    }
}

/// Получить ссылку на окно (только для чтения).
pub fn get(id: u32) -> Option<&'static Window> {
    unsafe {
        for i in 0..MAX_WINDOWS {
            if WINDOWS[i].id == id && (WINDOWS[i].state == WinState::Open || WINDOWS[i].minimized) {
                return Some(&WINDOWS[i]);
            }
        }
    }
    None
}

/// Получить mut ссылку на окно.
unsafe fn get_mut(id: u32) -> Option<&'static mut Window> {
    for i in 0..MAX_WINDOWS {
        if WINDOWS[i].id == id && WINDOWS[i].state == WinState::Open {
            return Some(&mut WINDOWS[i]);
        }
    }
    None
}

/// Сохранить фон окна (чтобы потом восстановить при закрытии/перетаскивании).
unsafe fn save_window_bg(slot: usize) {
    let win = &WINDOWS[slot];
    let x = win.x;
    let y = win.y;
    let w = win.w;
    let h = win.h;
    let max_w = 256u32.min(w);
    let max_h = 256u32.min(h);
    for row in 0..max_h as usize {
        for col in 0..max_w as usize {
            let idx = row * 256 + col;
            if idx >= BG_BUF_SIZE { break; }
            if let Some(Rgb(r, g, b)) = framebuffer::get(x + col as u32, y + row as u32) {
                WIN_BG_BUF[idx] = (r as u32) | ((g as u32) << 8) | ((b as u32) << 16);
            }
        }
    }
}

/// Восстановить фон окна.
unsafe fn restore_window_bg(slot: usize) {
    let win = &WINDOWS[slot];
    if win.w > 256 || win.h > 256 {
        // Для больших окон — просто заливаем фоном
        framebuffer::fill_rect(win.x, win.y, win.w, win.h, Rgb(238, 245, 239));
        return;
    }
    let max_w = 256u32.min(win.w);
    let max_h = 256u32.min(win.h);
    for row in 0..max_h as usize {
        for col in 0..max_w as usize {
            let idx = row * 256 + col;
            if idx >= BG_BUF_SIZE { break; }
            let packed = WIN_BG_BUF[idx];
            let r = (packed & 0xFF) as u8;
            let g = ((packed >> 8) & 0xFF) as u8;
            let b = ((packed >> 16) & 0xFF) as u8;
            framebuffer::put(win.x + col as u32, win.y + row as u32, Rgb(r, g, b));
        }
    }
}

/// Нарисовать тень окна.
unsafe fn draw_window_shadow(x: u32, y: u32, w: u32, h: u32) {
    let s = SHADOW_COLOR;
    // Bottom shadow
    for dy in 1..=6 {
        let alpha = ((6 - dy) * 12) as u8;
        for dx in -(WIN_RADIUS as i32)..=(w as i32 + WIN_RADIUS as i32) {
            let px = x as i32 + dx;
            let py = (y + h - 1 + dy) as i32;
            if px >= 0 && py >= 0 && px < framebuffer::width() as i32 && py < framebuffer::height() as i32 {
                framebuffer::put(px as u32, py as u32,
                    framebuffer::alpha_blend(
                        framebuffer::get(px as u32, py as u32).unwrap_or(Rgb(0,0,0)),
                        s, alpha,
                    )
                );
            }
        }
    }
    // Right shadow
    for dx in 1..=4 {
        let alpha = ((4 - dx) * 15) as u8;
        for dy in 0..h as i32 {
            let px = (x + w - 1 + dx as u32) as i32;
            let py = y as i32 + dy;
            if px >= 0 && py >= 0 && px < framebuffer::width() as i32 && py < framebuffer::height() as i32 {
                framebuffer::put(px as u32, py as u32,
                    framebuffer::alpha_blend(
                        framebuffer::get(px as u32, py as u32).unwrap_or(Rgb(0,0,0)),
                        s, alpha,
                    )
                );
            }
        }
    }
}

/// Нарисовать окно (рамку + заголовок).
pub unsafe fn draw_window(id: u32) {
    let slot = match find_by_id(id) {
        Some(s) => s,
        None => return,
    };
    let win = &WINDOWS[slot];

    // Glassmorphism backdrop
    framebuffer::draw_glass_background(win.x, win.y, win.w, win.h, 30);
    draw_window_shadow(win.x, win.y, win.w, win.h);

    // Title bar
    let title_bg = if TOP_Z > 0 && win.z == TOP_Z { Rgb(45, 55, 48) } else { TITLE_BG };
    framebuffer::fill_rect(win.x + WIN_RADIUS, win.y, win.w - WIN_RADIUS * 2, TITLE_H, title_bg);
    // Left/right title bar fill
    for dy in 0..TITLE_H {
        for dx in 0..WIN_RADIUS {
            let rx = WIN_RADIUS - dx;
            let ry = TITLE_H - dy;
            if rx * rx + ry * ry <= (WIN_RADIUS * WIN_RADIUS) as u32 {
                framebuffer::put(win.x + dx, win.y + dy, title_bg);
                framebuffer::put(win.x + win.w - 1 - dx, win.y + dy, title_bg);
                break;
            }
        }
    }

    // Title text
    let mut tx = win.x + 12;
    let title_y = win.y + (TITLE_H - 8) / 2;
    for &ch in win.title {
        framebuffer::draw_char_boot(tx, title_y, ch, TITLE_FG, 1);
        tx += 8;
    }

    // Minimize button (_) — between title and close
    let min_btn_x = win.x + win.w - TITLE_H * 2 - 4;
    let min_btn_y = win.y + 4;
    let min_color = if win.min_hover { MINIMIZE_BTN_HOVER } else { MINIMIZE_BTN };
    framebuffer::fill_rect(min_btn_x, min_btn_y, TITLE_H - 8, TITLE_H - 8, min_color);
    // _ glyph (a single line at bottom)
    framebuffer::fill_rect(min_btn_x + 4, min_btn_y + TITLE_H - 14, TITLE_H - 16, 2, TITLE_FG);

    // Close button (X) — right side of title bar
    let btn_x = win.x + win.w - TITLE_H - 4;
    let btn_y = win.y + 4;
    let btn_color = if win.close_hover { CLOSE_BTN_HOVER } else { CLOSE_BTN };
    framebuffer::fill_rect(btn_x, btn_y, TITLE_H - 8, TITLE_H - 8, btn_color);
    // X glyph
    let cx = btn_x + 4;
    let cy = btn_y + 4;
    framebuffer::put(cx, cy, TITLE_FG);
    framebuffer::put(cx + 6, cy, TITLE_FG);
    framebuffer::put(cx + 1, cy + 1, TITLE_FG);
    framebuffer::put(cx + 5, cy + 1, TITLE_FG);
    framebuffer::put(cx + 2, cy + 2, TITLE_FG);
    framebuffer::put(cx + 4, cy + 2, TITLE_FG);
    framebuffer::put(cx + 3, cy + 3, TITLE_FG);
    framebuffer::put(cx + 2, cy + 4, TITLE_FG);
    framebuffer::put(cx + 4, cy + 4, TITLE_FG);
    framebuffer::put(cx + 1, cy + 5, TITLE_FG);
    framebuffer::put(cx + 5, cy + 5, TITLE_FG);
    framebuffer::put(cx, cy + 6, TITLE_FG);
    framebuffer::put(cx + 6, cy + 6, TITLE_FG);

    // Window border
    graphics::draw_rect(win.x + WIN_RADIUS, win.y + TITLE_H, win.w - WIN_RADIUS * 2, win.h - TITLE_H,
        WIN_BORDER.0, WIN_BORDER.1, WIN_BORDER.2, false);

    // Content area background
    framebuffer::fill_rect(win.content_x, win.content_y, win.content_w, win.content_h, WIN_BG);

    // Resize handle (bottom-right corner)
    if win.w >= 20 && win.h >= 20 {
        let rx = win.x + win.w - 10;
        let ry = win.y + win.h - 10;
        // Small triangle pattern
        for i in 0..5 {
            for j in 0..i+1 {
                framebuffer::put(rx + j as u32 * 2, ry + i as u32 * 2, RESIZE_HANDLE_COLOR);
            }
        }
    }
}

/// Перерисовать все открытые окна (по z-order).
pub unsafe fn redraw_all() {
    // Сортируем по z-order и рисуем
    let mut order: [usize; MAX_WINDOWS] = [0; MAX_WINDOWS];
    let mut count = 0;
    for i in 0..MAX_WINDOWS {
        if WINDOWS[i].state == WinState::Open {
            order[count] = i;
            count += 1;
        }
    }
    // Bubble sort by z
    for i in 0..count {
        for j in i+1..count {
            if WINDOWS[order[i]].z > WINDOWS[order[j]].z {
                order.swap(i, j);
            }
        }
    }
    for i in 0..count {
        draw_window(WINDOWS[order[i]].id);
    }
}

/// Обработать событие мыши: возвращает (window_id, action).
/// action = 0: nothing, 1: title hit (drag), 2: close clicked, 3: content area hit,
///          4: minimize clicked, 5: resize hit
pub unsafe fn handle_mouse(mx: i32, my: i32, left_click: bool) -> (u32, u32) {
    // Find topmost window at this position
    let mut top_slot: Option<usize> = None;
    let mut top_z: u32 = 0;
    for i in 0..MAX_WINDOWS {
        if WINDOWS[i].state != WinState::Open { continue; }
        let w = &WINDOWS[i];
        if mx >= w.x as i32 && mx < (w.x + w.w) as i32
            && my >= w.y as i32 && my < (w.y + w.h) as i32
        {
            if w.z >= top_z {
                top_z = w.z;
                top_slot = Some(i);
            }
        }
    }

    let slot = match top_slot {
        Some(s) => s,
        None => return (0, 0),
    };
    let win = &WINDOWS[slot];

    // Minimize button
    let min_x = win.x + win.w - TITLE_H * 2 - 4;
    let min_y = win.y + 4;
    if mx >= min_x as i32 && mx < (min_x + TITLE_H - 8) as i32
        && my >= min_y as i32 && my < (min_y + TITLE_H - 8) as i32
    {
        if left_click {
            return (win.id, 4); // minimize
        }
        let wmut = &mut WINDOWS[slot];
        wmut.min_hover = true;
        return (win.id, 0);
    }

    // Close button
    let btn_x = win.x + win.w - TITLE_H - 4;
    let btn_y = win.y + 4;
    if mx >= btn_x as i32 && mx < (btn_x + TITLE_H - 8) as i32
        && my >= btn_y as i32 && my < (btn_y + TITLE_H - 8) as i32
    {
        if left_click {
            return (win.id, 2); // close
        }
        let wmut = &mut WINDOWS[slot];
        wmut.close_hover = true;
        return (win.id, 0);
    }

    // Resize handle (bottom-right 12x12 area)
    if mx >= (win.x + win.w - 12) as i32 && mx < (win.x + win.w) as i32
        && my >= (win.y + win.h - 12) as i32 && my < (win.y + win.h) as i32
    {
        if left_click {
            bring_to_front(win.id);
            let wmut = &mut WINDOWS[slot];
            wmut.resizing = true;
            wmut.resize_dir = 3; // corner resize
            return (win.id, 5);
        }
        return (win.id, 0);
    }

    // Check title bar
    if my >= win.y as i32 && my < (win.y + TITLE_H) as i32 {
        if left_click {
            bring_to_front(win.id);
            let wmut = &mut WINDOWS[slot];
            wmut.dragging = true;
            wmut.drag_off_x = mx - wmut.x as i32;
            wmut.drag_off_y = my - wmut.y as i32;
            return (win.id, 1);
        }
        return (win.id, 0);
    }

    // Content area
    if left_click {
        bring_to_front(win.id);
    }
    (win.id, 3)
}

/// Обновить позицию окна при перетаскивании.
pub unsafe fn drag_update(id: u32, mx: i32, my: i32) {
    if let Some(slot) = find_by_id(id) {
        if WINDOWS[slot].dragging {
            let old_x = WINDOWS[slot].x;
            let old_y = WINDOWS[slot].y;
            let new_x = (mx - WINDOWS[slot].drag_off_x).max(0).min(framebuffer::width() as i32 - WINDOWS[slot].w as i32) as u32;
            let new_y = (my - WINDOWS[slot].drag_off_y).max(0).min(framebuffer::height() as i32 - WINDOWS[slot].h as i32) as u32;

            if new_x != old_x || new_y != old_y {
                // Restore old position's bg
                WINDOWS[slot].x = old_x;
                WINDOWS[slot].y = old_y;
                restore_window_bg(slot);

                // Update position
                WINDOWS[slot].x = new_x;
                WINDOWS[slot].y = new_y;
                WINDOWS[slot].content_x = new_x + CONTENT_PAD;
                WINDOWS[slot].content_y = new_y + TITLE_H + CONTENT_PAD;

                // Save new bg + redraw
                save_window_bg(slot);
                draw_window(id);
            }
        }
    }
}

/// Обновить размер окна при ресайзе.
pub unsafe fn resize_update(id: u32, mx: i32, my: i32) {
    if let Some(slot) = find_by_id(id) {
        if !WINDOWS[slot].resizing { return; }
        let old_w = WINDOWS[slot].w;
        let old_h = WINDOWS[slot].h;
        let fb_w = framebuffer::width() as i32;
        let fb_h = framebuffer::height() as i32;

        // Restore old bg first
        restore_window_bg(slot);

        // New size with constraints
        let new_w = (mx as i32 - WINDOWS[slot].x as i32).max(160i32).min(fb_w - WINDOWS[slot].x as i32) as u32;
        let new_h = (my as i32 - WINDOWS[slot].y as i32).max(100i32).min(fb_h - WINDOWS[slot].y as i32) as u32;

        WINDOWS[slot].w = new_w;
        WINDOWS[slot].h = new_h;
        WINDOWS[slot].content_w = if new_w > CONTENT_PAD * 2 { new_w - CONTENT_PAD * 2 } else { 1 };
        WINDOWS[slot].content_h = if new_h > TITLE_H + CONTENT_PAD * 2 { new_h - TITLE_H - CONTENT_PAD * 2 } else { 1 };
        WINDOWS[slot].content_x = WINDOWS[slot].x + CONTENT_PAD;
        WINDOWS[slot].content_y = WINDOWS[slot].y + TITLE_H + CONTENT_PAD;

        // Save new bg + redraw
        save_window_bg(slot);
        draw_window(id);
        WINDOW_DIRTY[slot] = true;
    }
}

/// Завершить перетаскивание или ресайз.
pub unsafe fn drag_end() {
    for i in 0..MAX_WINDOWS {
        WINDOWS[i].dragging = false;
        WINDOWS[i].close_hover = false;
        WINDOWS[i].min_hover = false;
        WINDOWS[i].resizing = false;
        WINDOWS[i].resize_dir = 0;
    }
}

/// Получить content-область окна.
pub fn content_rect(id: u32) -> Option<(u32, u32, u32, u32)> {
    unsafe {
        for i in 0..MAX_WINDOWS {
            if WINDOWS[i].id == id && WINDOWS[i].state == WinState::Open {
                return Some((WINDOWS[i].content_x, WINDOWS[i].content_y, WINDOWS[i].content_w, WINDOWS[i].content_h));
            }
        }
    }
    None
}

/// Количество открытых окон (включая минимизированные).
pub fn open_count() -> usize {
    unsafe {
        let mut c = 0;
        for i in 0..MAX_WINDOWS {
            if WINDOWS[i].state == WinState::Open || WINDOWS[i].minimized { c += 1; }
        }
        c
    }
}

/// Количество не-минимизированных окон.
pub fn visible_count() -> usize {
    unsafe {
        let mut c = 0;
        for i in 0..MAX_WINDOWS {
            if WINDOWS[i].state == WinState::Open { c += 1; }
        }
        c
    }
}

/// Свернуть окно.
pub unsafe fn minimize(id: u32) {
    if let Some(slot) = find_by_id(id) {
        if WINDOWS[slot].state != WinState::Open { return; }
        WINDOWS[slot].saved_x = WINDOWS[slot].x;
        WINDOWS[slot].saved_y = WINDOWS[slot].y;
        WINDOWS[slot].minimized = true;
        WINDOWS[slot].state = WinState::Closed; // скрываем rendering
        restore_window_bg(slot);
    }
}

/// Восстановить окно из свернутого.
pub unsafe fn restore(id: u32) {
    if let Some(slot) = find_by_id(id) {
        if !WINDOWS[slot].minimized { return; }
        WINDOWS[slot].x = WINDOWS[slot].saved_x;
        WINDOWS[slot].y = WINDOWS[slot].saved_y;
        WINDOWS[slot].minimized = false;
        WINDOWS[slot].state = WinState::Open;
        TOP_Z += 1;
        WINDOWS[slot].z = TOP_Z;
        save_window_bg(slot);
        draw_window(id);
        WINDOW_DIRTY[slot] = true;
    }
}

/// Проверить, свернуто ли окно.
pub fn is_minimized(id: u32) -> bool {
    unsafe {
        for i in 0..MAX_WINDOWS {
            if WINDOWS[i].id == id { return WINDOWS[i].minimized; }
        }
    }
    false
}

/// Получить ID окна по индексу в списке.
pub fn window_id_at(idx: usize) -> Option<u32> {
    unsafe {
        let mut count = 0;
        for i in 0..MAX_WINDOWS {
            if WINDOWS[i].state == WinState::Open || WINDOWS[i].minimized {
                if count == idx {
                    return Some(WINDOWS[i].id);
                }
                count += 1;
            }
        }
    }
    None
}

// ═══════════════════════════════════════════════════════════════════
// App Render Infrastructure
// ═══════════════════════════════════════════════════════════════════

/// Тип приложения в окне.
#[derive(Clone, Copy, PartialEq)]
pub enum AppKind {
    None,
    Terminal,
    Snake,
    Paint,
    Files,
    Settings,
    Desktop,
}

/// Хранилище типа приложения для каждого окна.
static mut WINDOW_APPS: [AppKind; MAX_WINDOWS] = [AppKind::None; MAX_WINDOWS];
/// Флаг dirty — перерисовать содержимое окна.
static mut WINDOW_DIRTY: [bool; MAX_WINDOWS] = [false; MAX_WINDOWS];
/// Текстовые буферы для окон терминалов.
const TERM_BUF_ROWS: usize = 16;
const TERM_BUF_COLS: usize = 32;
static mut TERM_BUF: [[u8; TERM_BUF_COLS]; TERM_BUF_ROWS] = [[b' '; TERM_BUF_COLS]; TERM_BUF_ROWS];
static mut TERM_BUF_ROW: usize = 0;
static mut TERM_BUF_COL: usize = 0;
static mut TERM_BUF_WIN_ID: u32 = 0;

/// Установить тип приложения для окна.
pub unsafe fn set_app_kind(id: u32, kind: AppKind) {
    for i in 0..MAX_WINDOWS {
        if WINDOWS[i].id == id && WINDOWS[i].state == WinState::Open {
            WINDOW_APPS[i] = kind;
            WINDOW_DIRTY[i] = true;
            // Terminal init
            if kind == AppKind::Terminal {
                TERM_BUF_WIN_ID = id;
                TERM_BUF_ROW = 0;
                TERM_BUF_COL = 0;
                for r in 0..TERM_BUF_ROWS {
                    for c in 0..TERM_BUF_COLS {
                        TERM_BUF[r][c] = b' ';
                    }
                }
            }
            return;
        }
    }
}

/// Получить тип приложения для окна.
pub fn app_kind(id: u32) -> AppKind {
    unsafe {
        for i in 0..MAX_WINDOWS {
            if WINDOWS[i].id == id && WINDOWS[i].state == WinState::Open {
                return WINDOW_APPS[i];
            }
        }
    }
    AppKind::None
}

/// Пометить окно как требующее перерисовки содержимого.
pub unsafe fn set_dirty(id: u32) {
    for i in 0..MAX_WINDOWS {
        if WINDOWS[i].id == id && WINDOWS[i].state == WinState::Open {
            WINDOW_DIRTY[i] = true;
            return;
        }
    }
}

/// Записать символ в терминальное окно.
pub unsafe fn terminal_write(ch: u8) {
    if ch == b'\n' || ch == b'\r' {
        TERM_BUF_ROW += 1;
        TERM_BUF_COL = 0;
        if TERM_BUF_ROW >= TERM_BUF_ROWS {
            // Scroll
            for r in 1..TERM_BUF_ROWS {
                for c in 0..TERM_BUF_COLS {
                    TERM_BUF[r-1][c] = TERM_BUF[r][c];
                }
            }
            for c in 0..TERM_BUF_COLS {
                TERM_BUF[TERM_BUF_ROWS-1][c] = b' ';
            }
            TERM_BUF_ROW = TERM_BUF_ROWS - 1;
        }
    } else if ch == 0x08 || ch == 0x7F {
        if TERM_BUF_COL > 0 {
            TERM_BUF_COL -= 1;
            TERM_BUF[TERM_BUF_ROW][TERM_BUF_COL] = b' ';
        }
    } else if ch >= 0x20 && ch < 0x7F {
        if TERM_BUF_COL < TERM_BUF_COLS {
            TERM_BUF[TERM_BUF_ROW][TERM_BUF_COL] = ch;
            TERM_BUF_COL += 1;
        }
    }
    set_dirty(TERM_BUF_WIN_ID);
}

/// Записать строку в терминальное окно.
pub unsafe fn terminal_write_str(s: &[u8]) {
    for &ch in s {
        terminal_write(ch);
    }
}

/// Нарисовать содержимое указанного окна.
pub unsafe fn render_content(id: u32) {
    let slot = match find_by_id(id) {
        Some(s) => s,
        None => return,
    };
    if WINDOWS[slot].state != WinState::Open { return; }
    let kind = WINDOW_APPS[slot];
    let cx = WINDOWS[slot].content_x;
    let cy = WINDOWS[slot].content_y;
    let cw = WINDOWS[slot].content_w;
    let ch = WINDOWS[slot].content_h;

    match kind {
        AppKind::Terminal => render_terminal(cx, cy, cw, ch),
        AppKind::Snake => render_snake(cx, cy, cw, ch),
        AppKind::Paint => render_paint(cx, cy, cw, ch),
        AppKind::Files => render_files(cx, cy, cw, ch),
        AppKind::Settings => render_settings(cx, cy, cw, ch),
        AppKind::Desktop => render_desktop_info(cx, cy, cw, ch),
        AppKind::None => render_placeholder(cx, cy, cw, ch),
    }
    WINDOW_DIRTY[slot] = false;
}

/// Нарисовать содержимое всех открытых окон.
pub unsafe fn render_all_content() {
    let mut order: [usize; MAX_WINDOWS] = [0; MAX_WINDOWS];
    let mut count = 0;
    for i in 0..MAX_WINDOWS {
        if WINDOWS[i].state == WinState::Open {
            order[count] = i;
            count += 1;
        }
    }
    for i in 0..count {
        let id = WINDOWS[order[i]].id;
        render_content(id);
    }
}

// ═══════════════════════════════════════════════════════════════════
// Render implementations
// ═══════════════════════════════════════════════════════════════════

unsafe fn render_placeholder(cx: u32, cy: u32, cw: u32, ch: u32) {
    framebuffer::fill_rect(cx, cy, cw, ch, WIN_BG);
    let msg = b"Window";
    let mx = cx + (cw - (msg.len() as u32 * 8)) / 2;
    let my = cy + ch / 2 - 4;
    for (i, &ch) in msg.iter().enumerate() {
        framebuffer::draw_char_boot(mx + i as u32 * 8, my, ch, Rgb(150, 160, 150), 1);
    }
}

unsafe fn render_terminal(cx: u32, cy: u32, cw: u32, ch: u32) {
    framebuffer::fill_rect(cx, cy, cw, ch, Rgb(20, 25, 22));
    for row in 0..TERM_BUF_ROWS.min((ch / 10) as usize) {
        for col in 0..TERM_BUF_COLS.min((cw / 8) as usize) {
            let ch_byte = TERM_BUF[row][col];
            if ch_byte != b' ' {
                framebuffer::draw_char_boot(
                    cx + col as u32 * 8,
                    cy + row as u32 * 10,
                    ch_byte,
                    Rgb(127, 200, 150),
                    1,
                );
            }
        }
    }
    // Cursor
    let curs_x = cx + TERM_BUF_COL as u32 * 8;
    let curs_y = cy + TERM_BUF_ROW as u32 * 10;
    if curs_x < cx + cw && curs_y < cy + ch {
        framebuffer::fill_rect(curs_x, curs_y, 6, 9, Rgb(127, 200, 150));
    }
}

unsafe fn render_snake(cx: u32, cy: u32, cw: u32, ch: u32) {
    framebuffer::fill_rect(cx, cy, cw, ch, Rgb(15, 45, 25));
    // Grid
    let cell = 8u32;
    let cols = cw / cell;
    let rows = ch / cell;
    let ox = cx + (cw - cols * cell) / 2;
    let oy = cy + (ch - rows * cell) / 2;

    // Draw grid lines
    for r in 0..=rows {
        for c in 0..cols {
            framebuffer::put(ox + c * cell, oy + r * cell, Rgb(30, 70, 40));
        }
    }
    for c in 0..=cols {
        for r in 0..rows {
            framebuffer::put(ox + c * cell, oy + r * cell, Rgb(30, 70, 40));
        }
    }

    // Snake body (simple static pattern)
    let segs = [(4,3), (3,3), (2,3), (1,3)];
    for (i, &(sx, sy)) in segs.iter().enumerate() {
        let color = if i == 0 { Rgb(80, 220, 120) } else { Rgb(60, 180, 100) };
        if sx < cols && sy < rows {
            framebuffer::fill_rect(ox + sx * cell + 1, oy + sy * cell + 1, cell - 2, cell - 2, color);
        }
    }
    // Food
    framebuffer::fill_rect(ox + 7 * cell + 1, oy + 5 * cell + 1, cell - 2, cell - 2, Rgb(220, 60, 60));

    // Title
    let msg = b"Snake";
    let mx = cx + (cw - (msg.len() as u32 * 8)) / 2;
    framebuffer::draw_str(mx, cy + 2, msg, Rgb(127, 200, 150), 1);
}

unsafe fn render_paint(cx: u32, cy: u32, cw: u32, ch: u32) {
    framebuffer::fill_rect(cx, cy, cw, ch, Rgb(240, 245, 240));
    // Canvas area
    let canvas_h = ch.saturating_sub(30);
    framebuffer::fill_rect(cx + 4, cy + 4, cw - 8, canvas_h - 4, Rgb(255, 255, 255));
    // Border around canvas
    graphics::draw_rect(cx + 4, cy + 4, cw - 8, canvas_h - 4, 200, 200, 200, false);
    // Color palette
    let colors = [
        Rgb(0,0,0), Rgb(255,0,0), Rgb(0,200,0), Rgb(0,0,255),
        Rgb(255,255,0), Rgb(255,128,0), Rgb(255,0,255), Rgb(0,200,200),
    ];
    let palette_y = cy + canvas_h + 4;
    for (i, &color) in colors.iter().enumerate() {
        let px = cx + 4 + i as u32 * 18;
        framebuffer::fill_rect(px, palette_y, 16, 16, color);
        graphics::draw_rect(px, palette_y, 16, 16, 180, 180, 180, false);
    }
    let msg = b"Paint";
    let mx = cx + (cw - (msg.len() as u32 * 8)) / 2;
    framebuffer::draw_str(mx, cy + canvas_h + 22, msg, Rgb(80, 80, 80), 1);
}

unsafe fn render_files(cx: u32, cy: u32, cw: u32, ch: u32) {
    framebuffer::fill_rect(cx, cy, cw, ch, Rgb(30, 35, 40));
    // Path bar
    let path_bg = Rgb(20, 25, 30);
    framebuffer::fill_rect(cx + 4, cy + 4, cw - 8, 14, path_bg);
    let path = b"/home/user";
    for (i, &ch) in path.iter().enumerate() {
        framebuffer::draw_char_boot(cx + 6 + i as u32 * 8, cy + 5, ch, Rgb(100, 180, 220), 1);
    }
    // File entries
    let files: [&[u8]; 6] = [b"Documents", b"Pictures", b"Music", b"snake.elf", b"README.txt", b".."];
    let file_colors: [Rgb; 6] = [
        Rgb(127, 168, 140), Rgb(127, 168, 140), Rgb(127, 168, 140),
        Rgb(180, 180, 200), Rgb(180, 180, 200), Rgb(150, 150, 150),
    ];
    for (i, &name) in files.iter().enumerate() {
        if i as u32 * 14 + 24 > ch { break; }
        let fy = cy + 22 + i as u32 * 14;
        framebuffer::fill_rect(cx + 4, fy, cw - 8, 13, Rgb(35, 40, 45));
        let icon = if file_colors[i] == Rgb(127, 168, 140) { b" [D]" } else { b" [F]" };
        for (j, &ch) in icon.iter().enumerate() {
            framebuffer::draw_char_boot(cx + 6 + j as u32 * 8, fy + 2, ch, file_colors[i], 1);
        }
        let offset = icon.len() as u32 * 8 + 4;
        for (j, &ch) in name.iter().enumerate() {
            framebuffer::draw_char_boot(cx + 6 + offset + j as u32 * 8, fy + 2, ch, Rgb(200, 210, 200), 1);
        }
    }
}

unsafe fn render_settings(cx: u32, cy: u32, cw: u32, ch: u32) {
    framebuffer::fill_rect(cx, cy, cw, ch, Rgb(35, 40, 45));
    let items: [&[u8]; 5] = [b"Wallpaper", b"Terminal", b"Mouse", b"Display", b"About"];
    for (i, &name) in items.iter().enumerate() {
        let iy = cy + 6 + i as u32 * 22;
        if iy + 20 > cy + ch { break; }
        framebuffer::fill_rect(cx + 4, iy, cw - 8, 20, Rgb(45, 50, 55));
        for (j, &ch) in name.iter().enumerate() {
            framebuffer::draw_char_boot(cx + 10 + j as u32 * 8, iy + 5, ch, Rgb(200, 210, 200), 1);
        }
        // Arrow
        framebuffer::draw_char_boot(cx + cw - 16, iy + 5, b'>', Rgb(127, 168, 140), 1);
    }
}

unsafe fn render_desktop_info(cx: u32, cy: u32, cw: u32, ch: u32) {
    framebuffer::fill_rect(cx, cy, cw, ch, Rgb(25, 30, 35));
    let lines: [&[u8]; 4] = [
        b"PureOS Desktop",
        b"v0.4",
        b"64 processes max",
        b"Press Esc to exit",
    ];
    for (i, &line) in lines.iter().enumerate() {
        let mx = cx + (cw - (line.len() as u32 * 8)) / 2;
        let my = cy + 20 + i as u32 * 16;
        for (j, &ch) in line.iter().enumerate() {
            framebuffer::draw_char_boot(mx + j as u32 * 8, my, ch,
                if i == 0 { Rgb(127, 200, 150) } else { Rgb(160, 170, 160) }, 1);
        }
    }
}

/// Отправить символ в активное окно (Terminal).
pub unsafe fn send_key_to_active(ch: u8) -> bool {
    for i in 0..MAX_WINDOWS {
        if WINDOWS[i].state == WinState::Open && WINDOW_APPS[i] == AppKind::Terminal {
            if WINDOWS[i].z == TOP_Z {
                terminal_write(ch);
                return true;
            }
        }
    }
    false
}
