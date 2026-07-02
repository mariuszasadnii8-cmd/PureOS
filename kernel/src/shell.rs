//! Встроенная командная оболочка PureOS + Barrel REPL.
//!
//! Вывод через UEFI ConOut (терминал). Работает в процессе 0.

use crate::keyboard;
use crate::syscall;
use crate::terminal;

const CMD_BUF_SIZE: usize = 512;
static mut CMD_BUF: [u8; CMD_BUF_SIZE] = [0; CMD_BUF_SIZE];
static mut CMD_LEN: usize = 0;
static mut BARREL_MODE: bool = false;

/// Запустить оболочку. Не возвращается.
pub unsafe fn run() -> ! {
    if !terminal::is_init() {
        terminal::init();
    }

    terminal::clear();
    show_banner();

    CMD_LEN = 0;
    show_prompt();

    loop {
        // Безопасный idle-путь: показываем промпт и удерживаем экран живым,
        // не заходя в нестабильный UEFI ConIn-цикл на раннем старте.
        core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
    }
}

unsafe fn show_banner() {
    terminal::write(b"\n");
    terminal::write(b"  +----------------------------------------------------------+\n");
    terminal::write(b"  |  PUREOS  v0.3  [UEFI Terminal]                          |\n");
    terminal::write(b"  |  Type 'help' for commands  |  'barrel' for Barrel REPL   |\n");
    terminal::write(b"  +----------------------------------------------------------+\n");
    terminal::write(b"\n");
    // Serial backup
    crate::console::serial_puts(b"\n=== PUREOS v0.3 ===\n");
}

unsafe fn show_prompt() {
    if BARREL_MODE {
        terminal::write(b"barrel> ");
    } else {
        terminal::write(b"pureos$ ");
    }
}

unsafe fn handle_key(ch: u8) {
    match ch {
        0x7F | 0x08 => {
            if CMD_LEN > 0 {
                CMD_LEN -= 1;
                terminal::putchar(0x7F);
            }
        }
        b'\n' | b'\r' => {
            terminal::putchar(b'\n');
            if BARREL_MODE {
                execute_barrel();
            } else {
                execute_command();
            }
            CMD_LEN = 0;
            show_prompt();
        }
        0x1B => {
            while CMD_LEN > 0 {
                CMD_LEN -= 1;
                terminal::putchar(0x7F);
            }
        }
        _ if ch >= 0x20 && ch < 0x7F => {
            if CMD_LEN < CMD_BUF_SIZE - 1 {
                CMD_BUF[CMD_LEN] = ch;
                CMD_LEN += 1;
                terminal::putchar(ch);
            }
        }
        _ => {}
    }
}

unsafe fn execute_barrel() {
    if CMD_LEN == 0 { return; }
    let ptr = core::ptr::addr_of_mut!(CMD_BUF) as *const u8;
    let exit_cmd = CMD_LEN == 4
        && *ptr.add(0) == b'e' && *ptr.add(1) == b'x'
        && *ptr.add(2) == b'i' && *ptr.add(3) == b't';
    if exit_cmd {
        BARREL_MODE = false;
        terminal::write(b"Leaving Barrel REPL\n");
        return;
    }
    crate::barrel::exec(ptr, CMD_LEN);
}

unsafe fn execute_command() {
    if CMD_LEN == 0 { return; }

    let ptr = core::ptr::addr_of_mut!(CMD_BUF) as *const u8;
    let mut cmd_end = 0;
    while cmd_end < CMD_LEN && *ptr.add(cmd_end) != b' ' {
        cmd_end += 1;
    }

    let mut arg_start = cmd_end;
    while arg_start < CMD_LEN && *ptr.add(arg_start) == b' ' {
        arg_start += 1;
    }

    let args = core::slice::from_raw_parts(ptr.add(arg_start), CMD_LEN - arg_start);

    let is = |s: &[u8]| -> bool {
        if cmd_end != s.len() { return false; }
        for i in 0..cmd_end {
            if *ptr.add(i) != s[i] { return false; }
        }
        true
    };

    if is(b"help") { cmd_help(); }
    else if is(b"clear") { cmd_clear(); }
    else if is(b"ps") { cmd_ps(); }
    else if is(b"info") { cmd_info(); }
    else if is(b"ver") { cmd_version(); }
    else if is(b"echo") { cmd_echo(args); }
    else if is(b"demo") { cmd_demo(); }
    else if is(b"hex") { cmd_hex(args); }
    else if is(b"barrel") { cmd_barrel(); }
    else if is(b"reboot") { cmd_reboot(); }
    else if is(b"shutdown") { cmd_shutdown(); }
    else if is(b"exec") { cmd_exec(args); }
    else { unknown_command(); }
}

unsafe fn unknown_command() { terminal::write(b"unknown command. Type 'help'.\n"); }

unsafe fn cmd_help() {
    terminal::write(b"Built-in commands: help clear ps info ver echo demo hex\n");
    terminal::write(b"  barrel  - enter Barrel scripting REPL\n");
    terminal::write(b"  exec    - exec ELF from memory: exec <hex_addr> <hex_size>\n");
    terminal::write(b"  reboot  - reboot system (UEFI)\n");
    terminal::write(b"  shutdown- shutdown system (UEFI)\n");
}

unsafe fn cmd_clear() { terminal::clear(); }

unsafe fn cmd_ps() {
    terminal::write(b"  PID  STATE\n");
    for i in 0..crate::syscall::MAX_PROCESSES {
        let state = crate::syscall::PROCESS_TABLE[i].state;
        let id = crate::syscall::PROCESS_TABLE[i].id;
        if matches!(state, crate::syscall::ProcessState::Empty) { continue; }
        let state_name: &[u8] = match state {
            crate::syscall::ProcessState::Runnable => b"RUN",
            crate::syscall::ProcessState::BlockedOnSend { .. } => b"BLK-SND",
            crate::syscall::ProcessState::BlockedOnReceive => b"BLK-RCV",
            crate::syscall::ProcessState::BlockedOnReply { .. } => b"BLK-RPL",
            crate::syscall::ProcessState::Exited => b"EXIT",
            _ => b"??",
        };
        terminal::write(b"  "); terminal::write_num(id);
        terminal::write(b"     "); terminal::write(state_name);
        terminal::write(b"\n");
    }
}

unsafe fn cmd_info() {
    terminal::write(b"PureOS Crystal Kernel v0.3\n");
    terminal::write(b"Processes: "); terminal::write_num(crate::syscall::MAX_PROCESSES as u64);
    terminal::write(b" max\n");
    terminal::write(b"Input: UEFI Simple Text Input\n");
    terminal::write(b"Output: UEFI Simple Text Output\n");
    terminal::write(b"Script: Barrel (built-in)\n");
}

unsafe fn cmd_version() {
    terminal::write(b"PureOS Crystal v0.3.0\n");
    terminal::write(b"Immutable Ephemeral Kernel\n");
    terminal::write(b"UEFI-only | no legacy | Built: 2026-07\n");
}

unsafe fn cmd_echo(msg: &[u8]) { terminal::write(msg); terminal::write(b"\n"); }

unsafe fn cmd_demo() {
    let pid = crate::syscall::spawn_initial_user(crate::user_demo as *const () as u64);
    if pid >= 0 {
        terminal::write(b"spawned demo process, PID=");
        terminal::write_num(pid as u64); terminal::write(b"\n");
    } else {
        terminal::write(b"failed: "); terminal::write_num((-pid) as u64);
        terminal::write(b"\n");
    }
}

unsafe fn cmd_reboot() {
    terminal::write(b"rebooting...\n");
    crate::uefi::reset_system();
}

unsafe fn cmd_shutdown() {
    terminal::write(b"shutting down...\n");
    crate::uefi::shutdown();
}

unsafe fn cmd_barrel() {
    BARREL_MODE = true;
    terminal::write(b"Barrel REPL  (type 'exit' to return)\n");
}

unsafe fn cmd_exec(args: &[u8]) {
    let mut parts = args.split(|&c| c == b' ');
    let addr_str = parts.next().unwrap_or(b"");
    let size_str = parts.next().unwrap_or(b"");
    if addr_str.is_empty() || size_str.is_empty() {
        terminal::write(b"usage: exec <hex_addr> <hex_size>\n"); return;
    }
    let addr = parse_hex(addr_str);
    let size = parse_hex(size_str);
    if addr == 0 || size == 0 {
        terminal::write(b"invalid address or size\n"); return;
    }
    let pid = crate::elf::exec(addr, size);
    if pid >= 0 {
        terminal::write(b"loaded, PID="); terminal::write_num(pid as u64);
        terminal::write(b"\n");
    } else {
        terminal::write(b"exec failed: "); terminal::write_num((-pid) as u64);
        terminal::write(b"\n");
    }
}

unsafe fn cmd_hex(args: &[u8]) {
    let addr = parse_hex(args);
    if addr == 0 { terminal::write(b"usage: hex <addr>\n"); return; }
    let ptr = addr as *const u8;
    for row in 0..8 {
        terminal::write_hex(addr + row * 16); terminal::write(b": ");
        for col in 0..16 {
            let val = core::ptr::read_volatile(ptr.add((row * 16 + col) as usize));
            let hex = b"0123456789abcdef";
            terminal::putchar(hex[(val >> 4) as usize]);
            terminal::putchar(hex[(val & 0xF) as usize]);
            if col == 7 { terminal::putchar(b' '); }
            terminal::putchar(b' ');
        }
        terminal::write(b" |");
        for col in 0..16 {
            let val = core::ptr::read_volatile(ptr.add((row * 16 + col) as usize));
            terminal::putchar(if val >= 0x20 && val < 0x7F { val } else { b'.' });
        }
        terminal::write(b"|\n");
    }
}

fn parse_hex(s: &[u8]) -> u64 {
    if s.is_empty() { return 0; }
    let s = if s.len() > 2 && &s[..2] == b"0x" { &s[2..] } else { s };
    let mut val: u64 = 0;
    for &ch in s {
        let digit = match ch {
            b'0'..=b'9' => ch - b'0',
            b'a'..=b'f' => ch - b'a' + 10,
            b'A'..=b'F' => ch - b'A' + 10,
            _ => break,
        };
        val = (val << 4) | (digit as u64);
    }
    val
}
