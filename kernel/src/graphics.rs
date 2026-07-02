//! Графическая подсистема PureOS
//! Адаптивный интерфейс под любое разрешение экрана
//! Графические системные вызовы для рисования

use crate::framebuffer;

/// Информация о текущем разрешении экрана
pub struct ScreenInfo {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: u32,
}

/// Получить информацию о экране
pub fn get_screen_info() -> ScreenInfo {
    ScreenInfo {
        width: framebuffer::width(),
        height: framebuffer::height(),
        stride: framebuffer::stride(),
        format: 0, // TODO: добавить формат из framebuffer
    }
}

/// Проверить поддержку разрешения
pub fn is_resolution_supported(width: u32, height: u32) -> bool {
    let info = get_screen_info();
    info.width >= width && info.height >= height
}

/// Адаптивный масштаб шрифта под разрешение
pub fn calculate_font_scale() -> u32 {
    let info = get_screen_info();
    // Базовый масштаб для 1920x1080 = 2
    // Для меньших разрешений уменьшаем
    if info.width >= 1920 && info.height >= 1080 {
        2
    } else if info.width >= 1366 && info.height >= 768 {
        1
    } else if info.width >= 1024 && info.height >= 768 {
        1
    } else {
        1 // минимальный масштаб
    }
}

/// Графические примитивы
pub fn draw_pixel(x: u32, y: u32, r: u8, g: u8, b: u8) {
    framebuffer::put(x, y, framebuffer::Rgb(r, g, b));
}

pub fn draw_line(x1: u32, y1: u32, x2: u32, y2: u32, r: u8, g: u8, b: u8) {
    let color = framebuffer::Rgb(r, g, b);
    let dx = if x2 > x1 { x2 - x1 } else { x1 - x2 };
    let dy = if y2 > y1 { y2 - y1 } else { y1 - y2 };
    let sx = if x1 < x2 { 1 } else { -1i32 };
    let sy = if y1 < y2 { 1 } else { -1i32 };
    
    let mut err = if dx > dy { dx as i32 } else { -(dy as i32) } / 2;
    let mut x = x1 as i32;
    let mut y = y1 as i32;
    
    loop {
        framebuffer::put(x as u32, y as u32, color);
        if x == x2 as i32 && y == y2 as i32 { break; }
        let e2 = err;
        if e2 > -(dx as i32) {
            err -= dy as i32;
            x += sx;
        }
        if e2 < dy as i32 {
            err += dx as i32;
            y += sy;
        }
    }
}

pub fn draw_rect(x: u32, y: u32, w: u32, h: u32, r: u8, g: u8, b: u8, fill: bool) {
    let color = framebuffer::Rgb(r, g, b);
    if fill {
        framebuffer::fill_rect(x, y, w, h, color);
    } else {
        // Рисуем только рамку
        for i in 0..w {
            framebuffer::put(x + i, y, color);
            framebuffer::put(x + i, y + h - 1, color);
        }
        for i in 0..h {
            framebuffer::put(x, y + i, color);
            framebuffer::put(x + w - 1, y + i, color);
        }
    }
}

pub fn draw_circle(x: u32, y: u32, radius: u32, r: u8, g: u8, b: u8, fill: bool) {
    let color = framebuffer::Rgb(r, g, b);
    let mut x0 = 0i32;
    let mut y0 = radius as i32;
    let mut d = 3 - 2 * radius as i32;
    
    while y0 >= x0 {
        if fill {
            for i in (x as i32 - x0)..=(x as i32 + x0) {
                framebuffer::put((x as i32 + x0) as u32, (y as i32 - y0) as u32, color);
                framebuffer::put((x as i32 - x0) as u32, (y as i32 - y0) as u32, color);
                framebuffer::put((x as i32 + x0) as u32, (y as i32 + y0) as u32, color);
                framebuffer::put((x as i32 - x0) as u32, (y as i32 + y0) as u32, color);
                framebuffer::put((x as i32 + y0) as u32, (y as i32 - x0) as u32, color);
                framebuffer::put((x as i32 - y0) as u32, (y as i32 - x0) as u32, color);
                framebuffer::put((x as i32 + y0) as u32, (y as i32 + x0) as u32, color);
                framebuffer::put((x as i32 - y0) as u32, (y as i32 + x0) as u32, color);
            }
        } else {
            framebuffer::put((x as i32 + x0) as u32, (y as i32 - y0) as u32, color);
            framebuffer::put((x as i32 - x0) as u32, (y as i32 - y0) as u32, color);
            framebuffer::put((x as i32 + x0) as u32, (y as i32 + y0) as u32, color);
            framebuffer::put((x as i32 - x0) as u32, (y as i32 + y0) as u32, color);
            framebuffer::put((x as i32 + y0) as u32, (y as i32 - x0) as u32, color);
            framebuffer::put((x as i32 - y0) as u32, (y as i32 - x0) as u32, color);
            framebuffer::put((x as i32 + y0) as u32, (y as i32 + x0) as u32, color);
            framebuffer::put((x as i32 - y0) as u32, (y as i32 + x0) as u32, color);
        }
        x0 += 1;
        if d > 0 {
            y0 -= 1;
            d -= 4 * y0;
        }
        d += 4 * x0 + 2;
    }
}

/// Отрисовка изображения из буфера
pub fn draw_image(x: u32, y: u32, data: &[u8], width: u32, height: u32) {
    let screen_info = get_screen_info();
    for py in 0..height {
        for px in 0..width {
            let screen_x = x + px;
            let screen_y = y + py;
            if screen_x < screen_info.width && screen_y < screen_info.height {
                let idx = ((py * width + px) * 3) as usize;
                if idx + 2 < data.len() {
                    draw_pixel(screen_x, screen_y, data[idx], data[idx + 1], data[idx + 2]);
                }
            }
        }
    }
}

/// Очистка экрана с цветом
pub fn clear_screen(r: u8, g: u8, b: u8) {
    framebuffer::clear(framebuffer::Rgb(r, g, b));
}

/// Получить цвет по названию
fn eq_ignore_case(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b.iter()).all(|(&x, &y)| x.eq_ignore_ascii_case(&y))
}

pub fn color_from_name(name: &[u8]) -> Option<(u8, u8, u8)> {
    if eq_ignore_case(name, b"black") { Some((0, 0, 0)) }
    else if eq_ignore_case(name, b"white") { Some((255, 255, 255)) }
    else if eq_ignore_case(name, b"red") { Some((255, 0, 0)) }
    else if eq_ignore_case(name, b"green") { Some((0, 255, 0)) }
    else if eq_ignore_case(name, b"blue") { Some((0, 0, 255)) }
    else if eq_ignore_case(name, b"yellow") { Some((255, 255, 0)) }
    else if eq_ignore_case(name, b"cyan") { Some((0, 255, 255)) }
    else if eq_ignore_case(name, b"magenta") { Some((255, 0, 255)) }
    else if eq_ignore_case(name, b"gray") || eq_ignore_case(name, b"grey") { Some((128, 128, 128)) }
    else if eq_ignore_case(name, b"orange") { Some((255, 165, 0)) }
    else if eq_ignore_case(name, b"purple") { Some((128, 0, 128)) }
    else if eq_ignore_case(name, b"pink") { Some((255, 192, 203)) }
    else if eq_ignore_case(name, b"brown") { Some((165, 42, 42)) }
    else { None }
}
