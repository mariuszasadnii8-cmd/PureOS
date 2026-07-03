//! Snake — встроенная игра «змейка» в ядре PureOS.
//! Управление: W/A/S/D, Q — выход, R — рестарт.
//! Запуск: команда `snake` в оболочке процесса 0.
//!
//! Zero-Alloc: вся игровая логика на стеке, ни одной аллокации.

use crate::keyboard;
use crate::terminal;

const W: u8 = 28;
const H: u8 = 18;
const MAX_BODY: usize = 128;

#[derive(Copy, Clone, PartialEq)]
enum Dir { Up, Down, Left, Right }

/// Запустить игру. Возвращается при выходе (Q).
pub unsafe fn run() {
    terminal::clear();
    let mut app_running = true;

    while app_running {
        let mut body = [(0u8, 0u8); MAX_BODY];
        let mut len = 3u16;
        let mut dir = Dir::Right;
        let cx = W / 2;
        let cy = H / 2;
        body[0] = (cx, cy);
        body[1] = (cx - 1, cy);
        body[2] = (cx - 2, cy);
        let mut fx = 7u8;
        let mut fy = 7u8;
        let mut score = 0u32;

        'game: loop {
            keyboard::poll();
            while let Some(ch) = keyboard::read_key() {
                match ch {
                    b'w' | b'W' if dir != Dir::Down  => dir = Dir::Up,
                    b's' | b'S' if dir != Dir::Up    => dir = Dir::Down,
                    b'a' | b'A' if dir != Dir::Right => dir = Dir::Left,
                    b'd' | b'D' if dir != Dir::Left  => dir = Dir::Right,
                    b'q' | b'Q' => { app_running = false; break 'game; }
                    _ => {}
                }
            }
            if !app_running { break; }

            let (hx, hy) = body[0];
            let (nx, ny) = match dir {
                Dir::Up    => (hx, hy.wrapping_sub(1)),
                Dir::Down  => (hx, hy.wrapping_add(1)),
                Dir::Left  => (hx.wrapping_sub(1), hy),
                Dir::Right => (hx.wrapping_add(1), hy),
            };

            if nx >= W || ny >= H { break; }
            let mut self_hit = false;
            for i in 0..len as usize {
                if body[i] == (nx, ny) { self_hit = true; break; }
            }
            if self_hit { break; }

            for i in (1..len as usize).rev() {
                body[i] = body[i - 1];
            }
            body[0] = (nx, ny);

            if nx == fx && ny == fy {
                score += 1;
                if (len as usize) < MAX_BODY { len += 1; }
                'place: loop {
                    fx = fx.wrapping_add(7) % (W - 1) + 1;
                    fy = fy.wrapping_add(11) % (H - 1) + 1;
                    for i in 0..len as usize {
                        if body[i] == (fx, fy) { continue 'place; }
                    }
                    break;
                }
            }

            draw(&body[..len as usize], (fx, fy), score);

            for _ in 0..200_000 {
                core::arch::asm!("pause", options(nomem, nostack, preserves_flags));
            }
        }

        if !app_running { break; }
        terminal::clear();
        terminal::write(b"  +----------+\n");
        terminal::write(b"  | GAME OVER |\n");
        terminal::write(b"  +----------+\n");
        terminal::write(b"  Score: "); terminal::write_num(score as u64); terminal::write(b"\n");
        terminal::write(b"  Press R to restart, Q to quit\n");

        loop {
            keyboard::poll();
            if let Some(ch) = keyboard::read_key() {
                match ch {
                    b'r' | b'R' => break,
                    b'q' | b'Q' => { app_running = false; break; }
                    _ => {}
                }
            }
            core::arch::asm!("pause", options(nomem, nostack, preserves_flags));
        }
    }
    terminal::clear();
    terminal::write(b"snake exited\n");
}

unsafe fn draw(snake: &[(u8, u8)], food: (u8, u8), score: u32) {
    terminal::set_cursor(0, 0);
    terminal::write(b"+"); for _ in 0..W { terminal::putchar(b'-'); } terminal::write(b"+\n");
    for y in 0..H {
        terminal::putchar(b'|');
        for x in 0..W {
            let pt = (x, y);
            if pt == food {
                terminal::putchar(b'*');
            } else {
                let mut drawn = false;
                for (i, &seg) in snake.iter().enumerate() {
                    if seg == pt {
                        terminal::putchar(if i == 0 { b'@' } else { b'o' });
                        drawn = true;
                        break;
                    }
                }
                if !drawn { terminal::putchar(b' '); }
            }
        }
        terminal::write(b"|\n");
    }
    terminal::write(b"+"); for _ in 0..W { terminal::putchar(b'-'); } terminal::write(b"+\n");
    terminal::write(b"Score: "); terminal::write_num(score as u64);
    terminal::write(b"  WASD move, Q quit\n");
}
