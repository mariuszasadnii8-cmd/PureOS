//! Встроенная командная оболочка PureOS + Barrel REPL.
//!
//! Вывод через UEFI ConOut (терминал). Работает в процессе 0.

use crate::commands;
use crate::keyboard;
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

    // Главный REPL-цикл: опрашиваем UEFI-клавиатуру, скармливаем символы
    // обработчику, который редактирует CMD_BUF и по Enter выполняет команду.
    // Прерывания выключены (§5), поэтому это кооперативный polling, а не сон.
    loop {
        keyboard::poll();
        while let Some(ch) = keyboard::read_key() {
            handle_key(ch);
        }
        // Короткая пауза, чтобы не жечь CPU в плотном busy-loop.
        core::arch::asm!("pause", options(nomem, nostack, preserves_flags));
    }
}

unsafe fn show_banner() {
    terminal::write(b"\n");
    terminal::write(b"  +----------------------------------------------------------+\n");
    terminal::write(b"  |  PUREOS  v0.3 		                           |\n");
    terminal::write(b"  |  Type 'help' for commands  |  'barrel' for Barrel REPL   |\n");
    terminal::write(b"  +----------------------------------------------------------+\n");
    terminal::write(b"\n");
    // Serial backup
    crate::console::serial_puts(b"\n PUREOS v0.3 \n");
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

    // Add to history
    let cmd_slice = core::slice::from_raw_parts(ptr, CMD_LEN);
    commands::add_to_history(cmd_slice, CMD_LEN);

    let is = |s: &[u8]| -> bool {
        if cmd_end != s.len() { return false; }
        for i in 0..cmd_end {
            if *ptr.add(i) != s[i] { return false; }
        }
        true
    };

    // File system commands
    if is(b"help") { commands::cmd_help(); }
    else if is(b"pwd") { commands::cmd_pwd(); }
    else if is(b"ls") { commands::cmd_ls(args); }
    else if is(b"cd") { commands::cmd_cd(args); }
    else if is(b"mkdir") { commands::cmd_mkdir(args); }
    else if is(b"touch") { commands::cmd_touch(args); }
    else if is(b"rm") { commands::cmd_rm(args); }
    else if is(b"cp") { commands::cmd_cp(args); }
    else if is(b"mv") { commands::cmd_mv(args); }
    else if is(b"write") { commands::cmd_write(args); }
    else if is(b"tree") { commands::cmd_tree(args); }
    else if is(b"stat") { commands::cmd_stat(args); }
    // Text commands
    else if is(b"cat") { commands::cmd_cat(args); }
    else if is(b"head") { commands::cmd_head(args); }
    else if is(b"tail") { commands::cmd_tail(args); }
    else if is(b"grep") { commands::cmd_grep(args); }
    // System commands
    else if is(b"uname") { commands::cmd_uname(args); }
    else if is(b"uptime") { commands::cmd_uptime(); }
    else if is(b"whoami") { commands::cmd_whoami(); }
    else if is(b"id") { commands::cmd_id(); }
    else if is(b"ps") { cmd_ps(); }
    else if is(b"kill") { commands::cmd_kill(args); }
    else if is(b"df") { commands::cmd_df(args); }
    else if is(b"du") { commands::cmd_du(args); }
    else if is(b"free") { commands::cmd_free(args); }
    // Network commands
    else if is(b"ping") { commands::cmd_ping(args); }
    else if is(b"ifconfig") { commands::cmd_ifconfig(); }
    else if is(b"netstat") { commands::cmd_netstat(args); }
    // Permission commands
    else if is(b"sudo") { commands::cmd_sudo(args); }
    else if is(b"chmod") { commands::cmd_chmod(args); }
    else if is(b"chown") { commands::cmd_chown(args); }
    // Utility commands
    else if is(b"history") { commands::cmd_history(); }
    else if is(b"clear") { commands::cmd_clear(); }
    else if is(b"echo") { commands::cmd_echo(args); }
    else if is(b"exit") { commands::cmd_exit(); }
    else if is(b"man") { commands::cmd_man(args); }
    else if is(b"config") { commands::cmd_config(args); }
    else if is(b"install") { cmd_install(); }
    // PureOS specific commands
    else if is(b"info") { cmd_info(); }
    else if is(b"ver") { cmd_version(); }
    else if is(b"demo") { cmd_demo(); }
    else if is(b"hex") { cmd_hex(args); }
    else if is(b"barrel") { cmd_barrel(); }
    else if is(b"reboot") { cmd_reboot(); }
    else if is(b"shutdown") { cmd_shutdown(); }
    else if is(b"exec") { cmd_exec(args); }
    else if is(b"cc") { cmd_cc(args); }
    else if is(b"snake") { cmd_snake(); }
    else if is(b"test") { cmd_test(); }
    else if is(b"top") { cmd_top(); }
    else if is(b"run") { cmd_run(args); }
    else { unknown_command(); }
}

unsafe fn unknown_command() { terminal::write(b"unknown command. Type 'help'.\n"); }

unsafe fn cmd_help() {
    terminal::write(b"Built-in commands:\n");
    terminal::write(b"  help      - this help\n");
    terminal::write(b"  clear     - clear screen\n");
    terminal::write(b"  ps        - list processes\n");
    terminal::write(b"  info/ver  - system info\n");
    terminal::write(b"  echo/demo/hex\n");
    terminal::write(b"  barrel    - enter Barrel scripting REPL\n");
    terminal::write(b"  exec      - exec ELF: exec <hex_addr> <hex_size>\n");
    terminal::write(b"  cc        - compile Barrel to native ring3: cc <src>\n");
    terminal::write(b"  snake     - play Snake game\n");
    terminal::write(b"  test      - run kernel test suite\n");
    terminal::write(b"  top       - system monitor\n");
    terminal::write(b"  run       - run Barrel script from fs: run <path>\n");
    terminal::write(b"  reboot    - reboot system (UEFI)\n");
    terminal::write(b"  shutdown  - shutdown system (UEFI)\n");
    terminal::write(b"Type 'help' in shell for full command list.\n");
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

unsafe fn cmd_cc(src: &[u8]) {
    if src.is_empty() {
        terminal::write(b"usage: cc <barrel source>\n");
        terminal::write(b"  example: cc let x=7; let y=6; println x*y;\n");
        return;
    }
    let pid = crate::barrelc::compile_and_run(src.as_ptr(), src.len());
    if pid < 0 {
        terminal::write(b"cc error "); terminal::write_num((-pid) as u64);
        terminal::write(b"\n");
    } else {
        terminal::write(b"[cc] ring3 pid "); terminal::write_num(pid as u64);
        terminal::write(b" done\n");
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

unsafe fn cmd_snake() {
    terminal::write(b"Starting Snake... (WASD move, Q quit)\n");
    crate::snake_game::run();
}

unsafe fn cmd_test() {
    crate::test_runner::run();
}

unsafe fn cmd_top() {
    crate::sysmon::run();
}

unsafe fn cmd_run(args: &[u8]) {
    if args.is_empty() {
        terminal::write(b"usage: run <path>\n");
        terminal::write(b"  load and run a Barrel script from the filesystem\n");
        return;
    }
    let node = match crate::fs::resolve(args) {
        Some(n) => n,
        None => { terminal::write(b"run: file not found\n"); return; }
    };
    if crate::fs::kind(node) != crate::fs::Kind::File {
        terminal::write(b"run: not a file\n"); return;
    }
    let data = crate::fs::read(node);
    if data.is_empty() {
        terminal::write(b"run: empty file\n"); return;
    }
    terminal::write(b"Running: "); terminal::write(args); terminal::write(b"\n");
    let ptr = data.as_ptr();
    let len = data.len();
    crate::barrel::exec(ptr, len);
    terminal::write(b"\n[script done]\n");
}

pub(crate) fn parse_hex(s: &[u8]) -> u64 {
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

unsafe fn cmd_install() {
    terminal::write(b"Starting PureOS Installer...\n");
    terminal::write(b"Press ENTER to continue or ESC to cancel\n");
    
    loop {
        keyboard::poll();
        if let Some(ch) = keyboard::read_key() {
            match ch {
                b'\n' | b'\r' => {
                    crate::installer::run_installer();
                }
                0x1B => {
                    terminal::write(b"Installation cancelled.\n");
                    return;
                }
                _ => {}
            }
        }
        core::arch::asm!("pause", options(nomem, nostack, preserves_flags));
    }
}
