//! SysMon — встроенный системный монитор (аналог top).
//!
//! Запуск: команда `top` в оболочке.
//! Показывает: процессы, память, uptime, шедулер.
//! Zero-Alloc: все буферы на стеке.
//! Q — выход, R — обновить.

use crate::frame;
use crate::keyboard;
use crate::syscall;
use crate::terminal;

static mut TICK: u64 = 0;

/// Запустить монитор. Возвращается при нажатии Q.
pub unsafe fn run() {
    let mut running = true;
    terminal::clear();
    while running {
        TICK = TICK.wrapping_add(1);
        terminal::set_cursor(0, 0);

        // Заголовок
        terminal::write(b"+=======================================================+\n");
        terminal::write(b"| PUREOS System Monitor                         ");
        terminal::write_num(TICK);
        terminal::write(b" |\n");
        terminal::write(b"+=======================================================+\n");

        // ── Память ──
        let s = frame::stats();
        terminal::write(b"| Memory: ");
        terminal::write_num(s.total_frames);
        terminal::write(b" frames total, ");
        terminal::write_num(s.used_frames);
        terminal::write(b" used, ");
        terminal::write_num(s.free_frames);
        terminal::write(b" free");
        let pct = if s.total_frames > 0 { (s.used_frames * 100) / s.total_frames } else { 0 };
        terminal::write(b" (");
        terminal::write_num(pct);
        terminal::write(b"%)");
        for _ in 0..(30 - pct as usize / 2) { terminal::putchar(b' '); }
        terminal::write(b"|\n");

        // ── Процессы ──
        terminal::write(b"| Processes:\n");
        for i in 0..syscall::MAX_PROCESSES {
            let p = &syscall::PROCESS_TABLE[i];
            if matches!(p.state, syscall::ProcessState::Empty) { continue; }
            terminal::write(b"|   PID=");
            terminal::write_num(p.id);
            terminal::write(b"  state=");
            terminal::write(match p.state {
                syscall::ProcessState::Runnable => b"RUN      ",
                syscall::ProcessState::BlockedOnSend { .. } => b"BLK-SEND ",
                syscall::ProcessState::BlockedOnReceive => b"BLK-RECV ",
                syscall::ProcessState::BlockedOnReply { .. } => b"BLK-REPLY",
                syscall::ProcessState::Exited => b"EXIT     ",
                _ => b"??       ",
            });
            terminal::write(b"  entry=0x");
            terminal::write_hex(p.entry);
            terminal::write(b"\n");
        }

        // ── Системная информация ──
        terminal::write(b"| Scheduler: cooperative round-robin\n");
        terminal::write(b"| Kernel:    zero-alloc, no heap\n");
        terminal::write(b"| Processes: ");
        terminal::write_num(syscall::MAX_PROCESSES as u64);
        terminal::write(b" max\n");
        terminal::write(b"+=======================================================+\n");
        terminal::write(b"| Q=quit  R=refresh                                     |\n");
        terminal::write(b"+=======================================================+\n");

        // ── Ввод ──
        keyboard::poll();
        while let Some(ch) = keyboard::read_key() {
            match ch {
                b'q' | b'Q' => { running = false; }
                b'r' | b'R' => { /* будет перерисован на следующей итерации */ }
                _ => {}
            }
        }

        // ── Задержка ──
        for _ in 0..500_000 {
            core::arch::asm!("pause", options(nomem, nostack, preserves_flags));
        }
    }
    terminal::clear();
    terminal::write(b"sysmon exited\n");
}
