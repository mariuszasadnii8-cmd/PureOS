//! SysMon — встроенный системный монитор (аналог top).
//!
//! Запуск: команда `top` в оболочке.
//! Показывает: процессы, память, uptime, переключения контекста.
//! Q — выход, R — обновить.

use crate::frame;
use crate::keyboard;
use crate::syscall;
use crate::terminal;

/// Запустить монитор. Возвращается при нажатии Q.
pub unsafe fn run() {
    let mut running = true;
    terminal::clear();
    while running {
        terminal::set_cursor(0, 0);

        // ── Верхний колонтитул ──
        terminal::write(b"+=======================================================+\n");
        terminal::write(b"|  PUREOS System Monitor v0.4   (APIC ~100Hz)           |\n");
        terminal::write(b"+=======================================================+\n");

        // ── Uptime ──
        let ticks = syscall::get_tick_count();
        let secs = ticks / 100;
        let minutes = secs / 60;
        let hours = minutes / 60;
        let min_rem = minutes % 60;
        let sec_rem = secs % 60;
        terminal::write(b"| Uptime: ");
        if hours > 0 {
            terminal::write_num(hours); terminal::write(b"h ");
            terminal::write_num(min_rem); terminal::write(b"m ");
            terminal::write_num(sec_rem); terminal::write(b"s");
        } else {
            terminal::write_num(min_rem); terminal::write(b"m ");
            terminal::write_num(sec_rem); terminal::write(b"s");
        }
        terminal::write(b"                    Ticks: ");
        terminal::write_num(ticks);
        terminal::write(b"\n");

        // ── Память ──
        let s = frame::stats();
        terminal::write(b"| Memory: ");
        terminal::write_num(s.total_frames);
        terminal::write(b" frames (");
        terminal::write_num(s.total_bytes / (1024 * 1024));
        terminal::write(b" MiB), used ");
        terminal::write_num(s.used_frames);
        terminal::write(b" (");
        let pct = if s.total_frames > 0 { (s.used_frames * 100) / s.total_frames } else { 0 };
        terminal::write_num(pct);
        terminal::write(b"%), free ");
        terminal::write_num(s.free_frames);
        terminal::write(b"\n");

        // ── Процессы (только живые) ──
        terminal::write(b"| Processes:\n");
        let mut live_count = 0;
        for i in 0..syscall::MAX_PROCESSES {
            let p = &syscall::PROCESS_TABLE[i];
            if matches!(p.state, syscall::ProcessState::Empty) { continue; }
            live_count += 1;
            terminal::write(b"|   PID=");
            terminal::write_num(p.id);
            terminal::write(b"  ");
            // State with color hint
            match p.state {
                syscall::ProcessState::Runnable => terminal::write(b"RUN      "),
                syscall::ProcessState::BlockedOnSend { .. } => terminal::write(b"BLK-SEND "),
                syscall::ProcessState::BlockedOnReceive => terminal::write(b"BLK-RECV "),
                syscall::ProcessState::BlockedOnReply { .. } => terminal::write(b"BLK-REPLY"),
                syscall::ProcessState::Exited => terminal::write(b"EXIT     "),
                _ => terminal::write(b"??       "),
            }
            terminal::write(b" sched:");
            terminal::write_num(p.switch_count);
            terminal::write(b"x  0x");
            terminal::write_hex(p.entry);
            terminal::write(b"\n");
        }
        terminal::write(b"| Total live: ");
        terminal::write_num(live_count as u64);
        terminal::write(b"/");
        terminal::write_num(syscall::MAX_PROCESSES as u64);
        terminal::write(b"\n");

        // ── Системная информация ──
        terminal::write(b"| Scheduler: preemptive round-robin (APIC timer)\n");
        terminal::write(b"| Kernel:    zero-alloc, no heap, immutable crystal\n");
        terminal::write(b"| FS:        ramfs + blockfs (ATA PIO)\n");
        terminal::write(b"+=======================================================+\n");
        terminal::write(b"|  Q=quit  R=refresh                                    |\n");
        terminal::write(b"+=======================================================+\n");

        // ── Ввод ──
        keyboard::poll();
        while let Some(ch) = keyboard::read_key() {
            match ch {
                b'q' | b'Q' => { running = false; }
                b'r' | b'R' => {}
                _ => {}
            }
        }

        // ── Задержка ~500ms ──
        for _ in 0..500_000 {
            core::arch::asm!("pause", options(nomem, nostack, preserves_flags));
        }
    }
    terminal::clear();
    terminal::write(b"sysmon exited\n");
}
