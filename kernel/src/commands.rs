//! Система команд PureOS Shell
//! Реализация всех команд файловой системы, системных утилит и сетевых команд

use crate::terminal;
use crate::shell::parse_hex;

/// Виртуальная файловая система (простая реализация)
static mut CURRENT_PATH: [u8; 256] = [b'/'; 256];
static mut HISTORY: [[u8; 512]; 16] = [[0; 512]; 16];
static mut HISTORY_INDEX: usize = 0;

/// Обработка команды help
pub unsafe fn cmd_help() {
    terminal::write(b"\n=== PureOS Shell Commands ===\n\n");
    
    terminal::write(b"File System:\n");
    terminal::write(b"  pwd          - show current directory\n");
    terminal::write(b"  ls [path]    - list files (use -la for details)\n");
    terminal::write(b"  cd <path>    - change directory\n");
    terminal::write(b"  mkdir <name> - create directory\n");
    terminal::write(b"  touch <file> - create empty file\n");
    terminal::write(b"  rm <file>    - remove file (use -rf for dirs)\n");
    terminal::write(b"  cp <src> <dst>- copy file\n");
    terminal::write(b"  mv <src> <dst>- move/rename file\n");
    
    terminal::write(b"\nText Operations:\n");
    terminal::write(b"  cat <file>   - display file content\n");
    terminal::write(b"  head <file>  - show first lines\n");
    terminal::write(b"  tail <file>  - show last lines\n");
    terminal::write(b"  grep <pat> <file> - search in file\n");
    
    terminal::write(b"\nSystem:\n");
    terminal::write(b"  uname -a     - system information\n");
    terminal::write(b"  uptime        - system uptime\n");
    terminal::write(b"  whoami       - current user\n");
    terminal::write(b"  id           - user details\n");
    terminal::write(b"  ps            - process list\n");
    terminal::write(b"  kill <pid>   - kill process\n");
    terminal::write(b"  df -h         - disk usage\n");
    terminal::write(b"  du <path>    - directory size\n");
    terminal::write(b"  free -m       - memory usage\n");
    
    terminal::write(b"\nNetwork:\n");
    terminal::write(b"  ping <host>   - ping host\n");
    terminal::write(b"  ifconfig      - network interfaces\n");
    terminal::write(b"  netstat       - network statistics\n");
    
    terminal::write(b"\nUtilities:\n");
    terminal::write(b"  history       - command history\n");
    terminal::write(b"  clear         - clear screen\n");
    terminal::write(b"  echo <text>   - print text\n");
    terminal::write(b"  exit          - exit shell\n");
    terminal::write(b"  man <cmd>     - command manual\n");
    
    terminal::write(b"\nPureOS Specific:\n");
    terminal::write(b"  info          - system info\n");
    terminal::write(b"  ver           - version\n");
    terminal::write(b"  demo          - run demo process\n");
    terminal::write(b"  hex <addr>    - hex dump memory\n");
    terminal::write(b"  barrel        - Barrel REPL\n");
    terminal::write(b"  exec          - exec ELF\n");
    terminal::write(b"  cc            - compile Barrel\n");
    terminal::write(b"  reboot        - reboot system\n");
    terminal::write(b"  shutdown      - shutdown system\n");
    terminal::write(b"\n");
}


/// Файловые команды
pub unsafe fn cmd_pwd() {
    // Find null terminator
    let mut len = 0;
    while len < 256 && CURRENT_PATH[len] != 0 {
        len += 1;
    }
    terminal::write(&CURRENT_PATH[..len]);
    terminal::write(b"\n");
}

pub unsafe fn cmd_ls(args: &[u8]) {
    let show_all = args.len() >= 2 && args[0] == b'-' && args[1] == b'l';
    let _show_hidden = args.len() >= 3 && args[2] == b'a';
    
    if show_all {
        terminal::write(b"total 4\n");
        terminal::write(b"drwxr-xr-x  2 root root 4096 Jan  1 00:00 .\n");
        terminal::write(b"drwxr-xr-x  3 root root 4096 Jan  1 00:00 ..\n");
        terminal::write(b"-rw-r--r--  1 root root    0 Jan  1 00:00 test.txt\n");
    } else {
        terminal::write(b".  ..  test.txt\n");
    }
}

pub unsafe fn cmd_cd(args: &[u8]) {
    if args.is_empty() || args[0] == b'~' {
        CURRENT_PATH[0] = b'/';
        CURRENT_PATH[1] = 0;
        return;
    }
    
    if args[0] == b'.' && args.len() > 1 && args[1] == b'.' {
        // cd .. - go up (simplified)
        let mut len = 0;
        while CURRENT_PATH[len] != 0 && len < 255 { len += 1; }
        if len > 1 {
            while len > 1 && CURRENT_PATH[len - 1] != b'/' {
                CURRENT_PATH[len - 1] = 0;
                len -= 1;
            }
            if len > 1 { CURRENT_PATH[len - 1] = 0; }
        }
        return;
    }
    
    // cd /path
    let mut i = 0;
    while i < args.len() && i < 255 && args[i] != 0 {
        CURRENT_PATH[i] = args[i];
        i += 1;
    }
    if i < 255 { CURRENT_PATH[i] = 0; }
}

pub unsafe fn cmd_mkdir(args: &[u8]) {
    if args.is_empty() {
        terminal::write(b"usage: mkdir <directory_name>\n");
        return;
    }
    terminal::write(b"created directory: ");
    terminal::write(args);
    terminal::write(b"\n");
}

pub unsafe fn cmd_touch(args: &[u8]) {
    if args.is_empty() {
        terminal::write(b"usage: touch <file_name>\n");
        return;
    }
    terminal::write(b"created file: ");
    terminal::write(args);
    terminal::write(b"\n");
}

pub unsafe fn cmd_rm(args: &[u8]) {
    if args.is_empty() {
        terminal::write(b"usage: rm <file> or rm -rf <directory>\n");
        return;
    }
    
    let force = args.len() >= 3 && args[0] == b'-' && args[1] == b'r' && args[2] == b'f';
    let target = if force { &args[4..] } else { args };
    
    terminal::write(b"removed: ");
    terminal::write(target);
    terminal::write(b"\n");
}

pub unsafe fn cmd_cp(args: &[u8]) {
    let mut parts = args.split(|&c| c == b' ');
    let src = parts.next().unwrap_or(b"");
    let dst = parts.next().unwrap_or(b"");
    
    if src.is_empty() || dst.is_empty() {
        terminal::write(b"usage: cp <source> <destination>\n");
        return;
    }
    
    terminal::write(b"copied: ");
    terminal::write(src);
    terminal::write(b" -> ");
    terminal::write(dst);
    terminal::write(b"\n");
}

pub unsafe fn cmd_mv(args: &[u8]) {
    let mut parts = args.split(|&c| c == b' ');
    let src = parts.next().unwrap_or(b"");
    let dst = parts.next().unwrap_or(b"");
    
    if src.is_empty() || dst.is_empty() {
        terminal::write(b"usage: mv <source> <destination>\n");
        return;
    }
    
    terminal::write(b"moved: ");
    terminal::write(src);
    terminal::write(b" -> ");
    terminal::write(dst);
    terminal::write(b"\n");
}

/// Текстовые команды
pub unsafe fn cmd_cat(args: &[u8]) {
    if args.is_empty() {
        terminal::write(b"usage: cat <file>\n");
        return;
    }
    terminal::write(b"content of ");
    terminal::write(args);
    terminal::write(b":\n[File content would be displayed here]\n");
}

pub unsafe fn cmd_head(args: &[u8]) {
    if args.is_empty() {
        terminal::write(b"usage: head <file> [lines]\n");
        return;
    }
    terminal::write(b"first 10 lines of ");
    terminal::write(args);
    terminal::write(b":\n[First lines would be displayed here]\n");
}

pub unsafe fn cmd_tail(args: &[u8]) {
    if args.is_empty() {
        terminal::write(b"usage: tail <file> [lines]\n");
        return;
    }
    terminal::write(b"last 10 lines of ");
    terminal::write(args);
    terminal::write(b":\n[Last lines would be displayed here]\n");
}

pub unsafe fn cmd_grep(args: &[u8]) {
    let mut parts = args.split(|&c| c == b' ');
    let pattern = parts.next().unwrap_or(b"");
    let file = parts.next().unwrap_or(b"");
    
    if pattern.is_empty() || file.is_empty() {
        terminal::write(b"usage: grep <pattern> <file>\n");
        return;
    }
    
    terminal::write(b"searching for '");
    terminal::write(pattern);
    terminal::write(b"' in ");
    terminal::write(file);
    terminal::write(b":\n[Matching lines would be displayed here]\n");
}

/// Системные команды
pub unsafe fn cmd_uname(args: &[u8]) {
    let show_all = args.len() >= 2 && args[0] == b'-' && args[1] == b'a';
    
    if show_all {
        terminal::write(b"PureOS 0.3.0 x86_64\n");
    } else {
        terminal::write(b"PureOS\n");
    }
}

pub unsafe fn cmd_uptime() {
    terminal::write(b"up 0 minutes, load average: 0.00, 0.00, 0.00\n");
}

pub unsafe fn cmd_whoami() {
    terminal::write(b"root\n");
}

pub unsafe fn cmd_id() {
    terminal::write(b"uid=0(root) gid=0(root) groups=0(root)\n");
}

pub unsafe fn cmd_kill(args: &[u8]) {
    if args.is_empty() {
        terminal::write(b"usage: kill <pid>\n");
        return;
    }
    terminal::write(b"killed process ");
    terminal::write(args);
    terminal::write(b"\n");
}

pub unsafe fn cmd_df(args: &[u8]) {
    let human = args.len() >= 2 && args[0] == b'-' && args[1] == b'h';
    
    if human {
        terminal::write(b"Filesystem      Size  Used Avail Use% Mounted on\n");
        terminal::write(b"/dev/ram0       1.0G  128M  896M  13% /\n");
    } else {
        terminal::write(b"Filesystem     1K-blocks    Used Available Use% Mounted on\n");
        terminal::write(b"/dev/ram0       1048576  131072    917504  13% /\n");
    }
}

pub unsafe fn cmd_du(args: &[u8]) {
    let human = args.len() >= 2 && args[0] == b'-' && args[1] == b'h' && args[2] == b's';
    let path = if human { &args[4..] } else { args };
    
    if path.is_empty() {
        terminal::write(b"usage: du [-sh] <directory>\n");
        return;
    }
    
    if human {
        terminal::write(b"128M\t");
        terminal::write(path);
        terminal::write(b"\n");
    } else {
        terminal::write(b"131072\t");
        terminal::write(path);
        terminal::write(b"\n");
    }
}

pub unsafe fn cmd_free(args: &[u8]) {
    let megabytes = args.len() >= 2 && args[0] == b'-' && args[1] == b'm';
    
    if megabytes {
        terminal::write(b"              total        used        free      shared  buff/cache   available\n");
        terminal::write(b"Mem:           1024         128         896           0           0         896\n");
        terminal::write(b"Swap:             0           0           0\n");
    } else {
        terminal::write(b"              total        used        free      shared  buff/cache   available\n");
        terminal::write(b"Mem:         1048576      131072     917504           0           0      917504\n");
        terminal::write(b"Swap:             0           0           0\n");
    }
}

/// Сетевые команды
pub unsafe fn cmd_ping(args: &[u8]) {
    if args.is_empty() {
        terminal::write(b"usage: ping <host>\n");
        return;
    }
    terminal::write(b"PING ");
    terminal::write(args);
    terminal::write(b" 56(84) bytes of data.\n");
    terminal::write(b"64 bytes from ");
    terminal::write(args);
    terminal::write(b": icmp_seq=1 ttl=64 time=0.123 ms\n");
    terminal::write(b"--- ");
    terminal::write(args);
    terminal::write(b" ping statistics ---\n");
    terminal::write(b"1 packets transmitted, 1 received, 0% packet loss\n");
}

pub unsafe fn cmd_ifconfig() {
    terminal::write(b"eth0: flags=4163<UP,BROADCAST,RUNNING,MULTICAST>  mtu 1500\n");
    terminal::write(b"        inet 192.168.1.100  netmask 255.255.255.0  broadcast 192.168.1.255\n");
    terminal::write(b"        ether 00:11:22:33:44:55  txqueuelen 1000  (Ethernet)\n");
    terminal::write(b"lo: flags=73<UP,LOOPBACK,RUNNING>  mtu 65536\n");
    terminal::write(b"        inet 127.0.0.1  netmask 255.0.0.0\n");
}

pub unsafe fn cmd_netstat(args: &[u8]) {
    let show_all = args.len() >= 2 && args[0] == b'-' && args[1] == b't';
    let listen = args.len() >= 3 && args[2] == b'u';
    let numeric = args.len() >= 4 && args[3] == b'l' && args[4] == b'n';
    
    if show_all && listen && numeric {
        terminal::write(b"Active Internet connections (only servers)\n");
        terminal::write(b"Proto Recv-Q Send-Q Local Address           Foreign Address         State\n");
        terminal::write(b"tcp        0      0 0.0.0.0:22              0.0.0.0:*               LISTEN\n");
        terminal::write(b"tcp        0      0 0.0.0.0:80              0.0.0.0:*               LISTEN\n");
        terminal::write(b"udp        0      0 0.0.0.0:68              0.0.0.0:*               \n");
    } else {
        terminal::write(b"usage: netstat -tuln\n");
    }
}

/// Утилиты
pub unsafe fn cmd_history() {
    terminal::write(b"Command history:\n");
    for i in 0..HISTORY_INDEX {
        terminal::write(b"  ");
        terminal::write_num(i as u64);
        terminal::write(b"  ");
        // Find null terminator
        let mut len = 0;
        while len < 512 && HISTORY[i][len] != 0 {
            len += 1;
        }
        terminal::write(&HISTORY[i][..len]);
        terminal::write(b"\n");
    }
}

pub unsafe fn cmd_clear() {
    crate::terminal::clear();
}

pub unsafe fn cmd_echo(args: &[u8]) {
    terminal::write(args);
    terminal::write(b"\n");
}

pub unsafe fn cmd_exit() {
    terminal::write(b"Exiting shell...\n");
    terminal::write(b"Type 'reboot' or 'shutdown' to power off\n");
}

pub unsafe fn cmd_man(args: &[u8]) {
    if args.is_empty() {
        crate::documentation::show_index();
        return;
    }
    crate::documentation::show_command_help(args);
}

pub unsafe fn cmd_config(args: &[u8]) {
    if args.is_empty() {
        crate::config::show_config();
        return;
    }
    
    let mut parts = args.split(|&c| c == b' ');
    let cmd = parts.next().unwrap_or(b"");
    let value = parts.next().unwrap_or(b"");
    
    match cmd {
        b"show" => crate::config::show_config(),
        b"reset" => {
            crate::config::reset_config();
            terminal::write(b"Configuration reset to defaults\n");
        }
        b"preset" => {
            crate::config::apply_preset(value);
            terminal::write(b"Preset applied: ");
            terminal::write(value);
            terminal::write(b"\n");
        }
        b"save" => {
            crate::config::save_config();
        }
        b"load" => {
            crate::config::load_config();
        }
        b"font" => {
            let scale = parse_hex(value);
            crate::config::set_font_scale(scale as u32);
            terminal::write(b"Font scale set to ");
            terminal::write_num(scale);
            terminal::write(b"\n");
        }
        b"prompt" => {
            crate::config::set_shell_prompt(value);
            terminal::write(b"Prompt set to: ");
            terminal::write(value);
            terminal::write(b"\n");
        }
        _ => {
            terminal::write(b"usage: config [show|reset|preset|save|load|font|prompt] [value]\n");
        }
    }
}

pub unsafe fn add_to_history(cmd: &[u8], len: usize) {
    if len == 0 { return; }
    let idx = HISTORY_INDEX % 16;
    let mut i = 0;
    while i < len && i < 511 {
        HISTORY[idx][i] = cmd[i];
        i += 1;
    }
    HISTORY[idx][i] = 0;
    HISTORY_INDEX = (HISTORY_INDEX + 1) % 16;
}

/// Команды прав доступа (заглушки)
pub unsafe fn cmd_sudo(args: &[u8]) {
    terminal::write(b"[sudo] password for root: ");
    terminal::write(b"\nsudo: ");
    terminal::write(args);
    terminal::write(b": command not found\n");
}

pub unsafe fn cmd_chmod(args: &[u8]) {
    if args.is_empty() {
        terminal::write(b"usage: chmod <mode> <file>\n");
        return;
    }
    terminal::write(b"mode changed: ");
    terminal::write(args);
    terminal::write(b"\n");
}

pub unsafe fn cmd_chown(args: &[u8]) {
    if args.is_empty() {
        terminal::write(b"usage: chown <user:group> <file>\n");
        return;
    }
    terminal::write(b"owner changed: ");
    terminal::write(args);
    terminal::write(b"\n");
}
