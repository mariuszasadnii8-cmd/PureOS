//! Система команд PureOS Shell
//! Реализация всех команд файловой системы, системных утилит и сетевых команд

use crate::fs;
use crate::terminal;
use crate::shell::parse_hex;

static mut HISTORY: [[u8; 512]; 16] = [[0; 512]; 16];
static mut HISTORY_INDEX: usize = 0;

/// Обработка команды help — подробный вывод по категориям
pub unsafe fn cmd_help() {
    terminal::write(b"\n=== PureOS v0.4 Command Reference ===\n\n");

    terminal::write(b"--- File System ---\n");
    terminal::write(b"  pwd                   show current directory\n");
    terminal::write(b"  ls [-la] [path]       list directory contents\n");
    terminal::write(b"  cd <path>             change directory\n");
    terminal::write(b"  mkdir <path>          create directory\n");
    terminal::write(b"  touch <path>          create empty file\n");
    terminal::write(b"  rm [-rf] <path>       remove file or directory\n");
    terminal::write(b"  cp <src> <dst>        copy file\n");
    terminal::write(b"  mv <src> <dst>        move/rename file\n");
    terminal::write(b"  write <path> <text>   write text to file\n");
    terminal::write(b"  cat <path>            display file contents\n");
    terminal::write(b"  head <path> [n]       show first n lines\n");
    terminal::write(b"  tail <path> [n]       show last n lines\n");
    terminal::write(b"  grep <pattern> <file> search file for pattern\n");
    terminal::write(b"  stat <path>           file metadata\n");
    terminal::write(b"  tree [path]           directory tree\n");
    terminal::write(b"  du [-sh] <path>       directory size\n");
    terminal::write(b"  df [-h]               disk free space\n");

    terminal::write(b"\n--- System ---\n");
    terminal::write(b"  info                  detailed system info\n");
    terminal::write(b"  ver                   kernel version string\n");
    terminal::write(b"  uname [-a]            system identity\n");
    terminal::write(b"  uptime                system uptime\n");
    terminal::write(b"  whoami                current user name\n");
    terminal::write(b"  id                    user identity\n");
    terminal::write(b"  ps                    process list\n");
    terminal::write(b"  kill <pid>            terminate a process\n");
    terminal::write(b"  free                  memory usage (frame pool)\n");
    terminal::write(b"  top                   interactive system monitor\n");
    terminal::write(b"  hwinfo                full hardware probe\n");
    terminal::write(b"  hex <addr>            hex dump memory\n");
    terminal::write(b"  config [options]      system configuration\n");

    terminal::write(b"\n--- Execution ---\n");
    terminal::write(b"  barrel                enter Barrel REPL\n");
    terminal::write(b"  cc <source>           compile Barrel to ring3 ELF\n");
    terminal::write(b"  exec <addr> <size>    execute raw ELF from memory\n");
    terminal::write(b"  run <path>            run Barrel script from fs\n");
    terminal::write(b"  demo                  run demo user process\n");
    terminal::write(b"  test                  run built-in tests\n");

    terminal::write(b"\n--- Utilities ---\n");
    terminal::write(b"  clear                 clear terminal\n");
    terminal::write(b"  echo <text>           print text\n");
    terminal::write(b"  history               command history\n");
    terminal::write(b"  man [topic]           built-in documentation\n");
    terminal::write(b"  install               launch installer\n");

    terminal::write(b"\n--- Network ---\n");
    terminal::write(b"  net init              init RTL8139 NIC\n");
    terminal::write(b"  net dhcp              DHCP negotiation\n");
    terminal::write(b"  net dns <host>        resolve hostname\n");
    terminal::write(b"  net http <url>        HTTP GET\n");
    terminal::write(b"  net status            show network status\n");
    terminal::write(b"  ping <host>           ping via IP\n");
    terminal::write(b"  ifconfig              network interfaces\n");
    terminal::write(b"  netstat               network statistics\n");

    terminal::write(b"\n--- USB ---\n");
    terminal::write(b"  usb list              list USB devices\n");
    terminal::write(b"  usb test              interactive keyboard test\n");
    terminal::write(b"  usb scan              force re-enumeration\n");

    terminal::write(b"\n--- Images ---\n");
    terminal::write(b"  imgview <file>        display BMP\n");
    terminal::write(b"  jpeg <file>           display JPEG\n");
    terminal::write(b"  gif <file>            display GIF (animated)\n");

    terminal::write(b"\n--- Graphics ---\n");
    terminal::write(b"  wallpaper [name|load] desktop wallpaper\n");
    terminal::write(b"  theme [name]          terminal color theme\n");
    terminal::write(b"  font [name|n]         set font/scale\n");
    terminal::write(b"  desktop               desktop layer manager\n");

    terminal::write(b"\n--- Visual ---\n");
    terminal::write(b"  glass [0|1|2|3]       glassmorphism effect\n");
    terminal::write(b"  rainbow [text]        rainbow-colored text\n");
    terminal::write(b"  wallpaper [name|load] desktop wallpaper\n");
    terminal::write(b"  theme [name]          terminal color theme\n");
    terminal::write(b"  font [name|n]         set font/scale\n");
    terminal::write(b"  desktop               desktop layer manager\n");

    terminal::write(b"\n--- Demos / Fun ---\n");
    terminal::write(b"  snake                 classic Snake game (arrows/WASD)\n");
    terminal::write(b"  matrix                Matrix digital rain\n");
    terminal::write(b"  mandelbrot            Mandelbrot fractal\n");
    terminal::write(b"  starfield             3D starfield simulation\n");
    terminal::write(b"  fire                  classic fire effect\n");
    terminal::write(b"  bounce                bouncing PUREOS logo\n");
    terminal::write(b"  rain                  rain effect\n");
    terminal::write(b"  snow                  snowfall effect\n");
    terminal::write(b"  kaleidoscope          colorful kaleidoscope\n");
    terminal::write(b"  plasma                colorful plasma effect\n");
    terminal::write(b"  gol                   Conway's Game of Life\n");
    terminal::write(b"  tunnel                psychedelic tunnel\n");
    terminal::write(b"  cube3d                rotating 3D wireframe cube\n");
    terminal::write(b"  donut                 rotating 3D torus (z-buffer)\n");
    terminal::write(b"  demoloop              run all demos in a loop\n");
    terminal::write(b"  play [notes]          play sound (music)\n");
    terminal::write(b"  volume [0..100]       sound volume control\n");
    terminal::write(b"  sound [on|off]        toggle sound\n");
    terminal::write(b"  beep [freq] [ms]      PC speaker beep\n");
    terminal::write(b"  clock                 read RTC clock\n");

    terminal::write(b"\n--- Permissions ---\n");
    terminal::write(b"  sudo <cmd>            run as superuser\n");
    terminal::write(b"  chmod <mode> <file>   change permissions\n");
    terminal::write(b"  chown <user> <file>   change owner\n");

    terminal::write(b"\n--- Power ---\n");
    terminal::write(b"  reboot                reboot via UEFI\n");
    terminal::write(b"  shutdown              power off via UEFI\n");

    terminal::write(b"\nUse 'man <command>' for detailed help on any command.\n");
    terminal::write(b"Use 'man' alone for documentation index.\n\n");
}


/// Файловые команды — работают на реальной in-RAM ФС (`fs.rs`).
pub unsafe fn cmd_pwd() {
    let mut buf = [0u8; 256];
    let len = fs::path_of(fs::cwd(), &mut buf);
    terminal::write(&buf[..len]);
    terminal::write(b"\n");
}

pub unsafe fn cmd_ls(args: &[u8]) {
    // Разобрать флаг -l и опциональный путь.
    let long = args.len() >= 2 && args[0] == b'-' && args[1] == b'l';
    let path = if long {
        let rest = &args[2..];
        let mut i = 0;
        while i < rest.len() && rest[i] == b' ' { i += 1; }
        &rest[i..]
    } else {
        args
    };

    let dir = if path.is_empty() {
        fs::cwd()
    } else {
        match fs::resolve(path) {
            Some(d) => d,
            None => { terminal::write(b"ls: no such path\n"); return; }
        }
    };
    if fs::kind(dir) != fs::Kind::Dir {
        // Это файл — просто напечатать его имя.
        terminal::write(fs::node_name(dir));
        terminal::write(b"\n");
        return;
    }

    let mut any = false;
    fs::for_each_child(dir, |child| {
        any = true;
        if long {
            match fs::kind(child) {
                fs::Kind::Dir => terminal::write(b"drwxr-xr-x  root root "),
                _ => terminal::write(b"-rw-r--r--  root root "),
            }
            terminal::write_num(fs::size_of(child) as u64);
            terminal::write(b"  ");
            terminal::write(fs::node_name(child));
            if fs::kind(child) == fs::Kind::Dir { terminal::write(b"/"); }
            terminal::write(b"\n");
        } else {
            terminal::write(fs::node_name(child));
            if fs::kind(child) == fs::Kind::Dir { terminal::write(b"/"); }
            terminal::write(b"  ");
        }
    });
    if !long {
        terminal::write(b"\n");
    }
    let _ = any;
}

pub unsafe fn cmd_cd(args: &[u8]) {
    let target = if args.is_empty() || args[0] == b'~' { b"/" as &[u8] } else { args };
    match fs::resolve(target) {
        Some(node) if fs::kind(node) == fs::Kind::Dir => fs::set_cwd(node),
        Some(_) => terminal::write(b"cd: not a directory\n"),
        None => terminal::write(b"cd: no such directory\n"),
    }
}

pub unsafe fn cmd_mkdir(args: &[u8]) {
    if args.is_empty() {
        terminal::write(b"usage: mkdir <path>\n");
        return;
    }
    match fs::resolve_parent(args) {
        Some((parent, leaf)) => {
            if fs::mkdir(parent, leaf).is_none() {
                terminal::write(b"mkdir: cannot create (exists or full)\n");
            }
        }
        None => terminal::write(b"mkdir: invalid path\n"),
    }
}

pub unsafe fn cmd_touch(args: &[u8]) {
    if args.is_empty() {
        terminal::write(b"usage: touch <path>\n");
        return;
    }
    match fs::resolve_parent(args) {
        Some((parent, leaf)) => {
            if fs::create_file(parent, leaf).is_none() {
                terminal::write(b"touch: cannot create\n");
            }
        }
        None => terminal::write(b"touch: invalid path\n"),
    }
}

pub unsafe fn cmd_rm(args: &[u8]) {
    if args.is_empty() {
        terminal::write(b"usage: rm [-rf] <path>\n");
        return;
    }
    let recursive = args.len() >= 3 && args[0] == b'-' && args[1] == b'r';
    let target = if args[0] == b'-' {
        // Пропустить флаг и пробелы.
        let mut i = 1;
        while i < args.len() && args[i] != b' ' { i += 1; }
        while i < args.len() && args[i] == b' ' { i += 1; }
        &args[i..]
    } else {
        args
    };
    let node = match fs::resolve(target) {
        Some(n) => n,
        None => { terminal::write(b"rm: no such file\n"); return; }
    };
    if recursive && fs::kind(node) == fs::Kind::Dir {
        rm_recursive(node);
    }
    if !fs::unlink(node) {
        terminal::write(b"rm: not empty (use -rf) or root\n");
    }
}

unsafe fn rm_recursive(dir: u16) {
    // Собрать детей, удалить рекурсивно. Собираем индексы во временный буфер.
    loop {
        let mut child = 0u16;
        let mut found = false;
        fs::for_each_child(dir, |c| {
            if !found { child = c; found = true; }
        });
        if !found { break; }
        if fs::kind(child) == fs::Kind::Dir {
            rm_recursive(child);
        }
        let _ = fs::unlink(child);
    }
}

pub unsafe fn cmd_cp(args: &[u8]) {
    let mut parts = args.split(|&c| c == b' ');
    let src = parts.next().unwrap_or(b"");
    let dst = parts.next().unwrap_or(b"");
    if src.is_empty() || dst.is_empty() {
        terminal::write(b"usage: cp <source> <destination>\n");
        return;
    }
    let src_node = match fs::resolve(src) {
        Some(n) if fs::kind(n) == fs::Kind::File => n,
        _ => { terminal::write(b"cp: source not a file\n"); return; }
    };
    let data = fs::read(src_node);
    match fs::resolve_parent(dst) {
        Some((parent, leaf)) => {
            if let Some(new) = fs::create_file(parent, leaf) {
                let _ = fs::write(new, data);
            } else {
                terminal::write(b"cp: cannot create destination\n");
            }
        }
        None => terminal::write(b"cp: invalid destination\n"),
    }
}

pub unsafe fn cmd_mv(args: &[u8]) {
    // Реализовано как cp + rm источника.
    let mut parts = args.split(|&c| c == b' ');
    let src = parts.next().unwrap_or(b"");
    let dst = parts.next().unwrap_or(b"");
    if src.is_empty() || dst.is_empty() {
        terminal::write(b"usage: mv <source> <destination>\n");
        return;
    }
    let src_node = match fs::resolve(src) {
        Some(n) if fs::kind(n) == fs::Kind::File => n,
        _ => { terminal::write(b"mv: source not a file\n"); return; }
    };
    let data = fs::read(src_node);
    match fs::resolve_parent(dst) {
        Some((parent, leaf)) => {
            if let Some(new) = fs::create_file(parent, leaf) {
                let _ = fs::write(new, data);
                let _ = fs::unlink(src_node);
            } else {
                terminal::write(b"mv: cannot create destination\n");
            }
        }
        None => terminal::write(b"mv: invalid destination\n"),
    }
}

/// Записать текст в файл (перезапись): write <path> <text...>
pub unsafe fn cmd_write(args: &[u8]) {
    let mut i = 0;
    while i < args.len() && args[i] != b' ' { i += 1; }
    let path = &args[..i];
    while i < args.len() && args[i] == b' ' { i += 1; }
    let text = &args[i..];
    if path.is_empty() {
        terminal::write(b"usage: write <path> <text>\n");
        return;
    }
    let node = match fs::resolve_parent(path) {
        Some((parent, leaf)) => match fs::create_file(parent, leaf) {
            Some(n) => n,
            None => { terminal::write(b"write: cannot open\n"); return; }
        },
        None => { terminal::write(b"write: invalid path\n"); return; }
    };
    if !fs::write(node, text) {
        terminal::write(b"write: fs data pool full\n");
    }
}

/// Дерево каталогов от текущего или заданного пути.
pub unsafe fn cmd_tree(args: &[u8]) {
    let dir = if args.is_empty() { fs::cwd() } else {
        match fs::resolve(args) { Some(d) => d, None => { terminal::write(b"tree: no such path\n"); return; } }
    };
    terminal::write(fs::node_name(dir));
    terminal::write(b"\n");
    tree_walk(dir, 1);
}

unsafe fn tree_walk(dir: u16, depth: u32) {
    if fs::kind(dir) != fs::Kind::Dir || depth > 8 { return; }
    fs::for_each_child(dir, |child| {
        for _ in 0..depth { terminal::write(b"  "); }
        terminal::write(b"|- ");
        terminal::write(fs::node_name(child));
        if fs::kind(child) == fs::Kind::Dir { terminal::write(b"/"); }
        terminal::write(b"\n");
        if fs::kind(child) == fs::Kind::Dir {
            tree_walk(child, depth + 1);
        }
    });
}

/// stat <path>
pub unsafe fn cmd_stat(args: &[u8]) {
    if args.is_empty() { terminal::write(b"usage: stat <path>\n"); return; }
    let node = match fs::resolve(args) {
        Some(n) => n,
        None => { terminal::write(b"stat: no such file\n"); return; }
    };
    terminal::write(b"  Name: "); terminal::write(fs::node_name(node)); terminal::write(b"\n");
    terminal::write(b"  Type: ");
    match fs::kind(node) {
        fs::Kind::Dir => terminal::write(b"directory\n"),
        fs::Kind::File => terminal::write(b"file\n"),
        _ => terminal::write(b"free\n"),
    }
    terminal::write(b"  Size: "); terminal::write_num(fs::size_of(node) as u64); terminal::write(b" bytes\n");
}

/// Текстовые команды
pub unsafe fn cmd_cat(args: &[u8]) {
    if args.is_empty() {
        terminal::write(b"usage: cat <file>\n");
        return;
    }
    match fs::resolve(args) {
        Some(n) if fs::kind(n) == fs::Kind::File => {
            terminal::write(fs::read(n));
        }
        Some(_) => terminal::write(b"cat: is a directory\n"),
        None => terminal::write(b"cat: no such file\n"),
    }
}

pub unsafe fn cmd_head(args: &[u8]) {
    // Parse: head <file> [n]
    let mut parts = args.split(|&c| c == b' ');
    let path = parts.next().unwrap_or(b"");
    let count_str = parts.next().unwrap_or(b"");
    if path.is_empty() {
        terminal::write(b"usage: head <file> [lines]\n");
        return;
    }
    let n: usize = if !count_str.is_empty() {
        let mut v = 0usize;
        for &ch in count_str {
            if ch >= b'0' && ch <= b'9' { v = v * 10 + (ch - b'0') as usize; }
            else { break; }
        }
        if v == 0 { 10 } else { v }
    } else {
        10
    };
    let node = match fs::resolve(path) {
        Some(n) if fs::kind(n) == fs::Kind::File => n,
        Some(_) => { terminal::write(b"head: is a directory\n"); return; }
        None => { terminal::write(b"head: no such file\n"); return; }
    };
    let data = fs::read(node);
    if data.is_empty() { return; }
    let mut lines = 0;
    let mut i = 0;
    while i < data.len() && lines < n {
        let line_start = i;
        while i < data.len() && data[i] != b'\n' { i += 1; }
        terminal::write(&data[line_start..i]);
        terminal::write(b"\n");
        lines += 1;
        if i < data.len() { i += 1; } // skip \n
    }
}

pub unsafe fn cmd_tail(args: &[u8]) {
    let mut parts = args.split(|&c| c == b' ');
    let path = parts.next().unwrap_or(b"");
    let count_str = parts.next().unwrap_or(b"");
    if path.is_empty() {
        terminal::write(b"usage: tail <file> [lines]\n");
        return;
    }
    let n: usize = if !count_str.is_empty() {
        let mut v = 0usize;
        for &ch in count_str {
            if ch >= b'0' && ch <= b'9' { v = v * 10 + (ch - b'0') as usize; }
            else { break; }
        }
        if v == 0 { 10 } else { v }
    } else {
        10
    };
    let node = match fs::resolve(path) {
        Some(n) if fs::kind(n) == fs::Kind::File => n,
        Some(_) => { terminal::write(b"tail: is a directory\n"); return; }
        None => { terminal::write(b"tail: no such file\n"); return; }
    };
    let data = fs::read(node);
    if data.is_empty() { return; }
    // Count total lines
    let mut total_lines = 0;
    for &b in data.iter() {
        if b == b'\n' { total_lines += 1; }
    }
    if data.len() > 0 && data[data.len()-1] != b'\n' { total_lines += 1; }
    // Skip to the last n lines
    let skip = if total_lines > n { total_lines - n } else { 0 };
    let mut lines_seen = 0;
    let mut i = 0;
    while i < data.len() {
        let line_start = i;
        while i < data.len() && data[i] != b'\n' { i += 1; }
        if lines_seen >= skip {
            terminal::write(&data[line_start..i]);
            terminal::write(b"\n");
        }
        lines_seen += 1;
        if i < data.len() { i += 1; }
    }
}

pub unsafe fn cmd_grep(args: &[u8]) {
    let mut parts = args.split(|&c| c == b' ');
    let pattern = parts.next().unwrap_or(b"");
    let file = parts.next().unwrap_or(b"");
    
    if pattern.is_empty() || file.is_empty() {
        terminal::write(b"usage: grep <pattern> <file>\n");
        return;
    }
    
    let node = match fs::resolve(file) {
        Some(n) if fs::kind(n) == fs::Kind::File => n,
        Some(_) => { terminal::write(b"grep: is a directory\n"); return; }
        None => { terminal::write(b"grep: no such file\n"); return; }
    };
    let data = fs::read(node);
    if data.is_empty() { return; }
    let mut matches = 0;
    let mut i = 0;
    while i < data.len() {
        let line_start = i;
        while i < data.len() && data[i] != b'\n' { i += 1; }
        let line = &data[line_start..i];
        // Simple substring search (case-sensitive)
        if line.len() >= pattern.len() {
            let mut found = false;
            for w in line.windows(pattern.len()) {
                if w == pattern { found = true; break; }
            }
            if found {
                terminal::write(line);
                terminal::write(b"\n");
                matches += 1;
            }
        }
        if i < data.len() { i += 1; }
    }
    if matches == 0 {
        terminal::write(b"(no matches)\n");
    }
}

/// Системные команды
pub unsafe fn cmd_uname(args: &[u8]) {
    let show_all = args.len() >= 2 && args[0] == b'-' && args[1] == b'a';
    
    if show_all {
        terminal::write(b"PureOS 0.4.0 x86_64\n");
    } else {
        terminal::write(b"PureOS\n");
    }
}

pub unsafe fn cmd_uptime() {
    let ticks = crate::syscall::get_tick_count();
    // APIC timer ~100 Hz, so ticks/100 = seconds
    let secs = ticks / 100;
    let minutes = secs / 60;
    let hours = minutes / 60;
    let min_rem = minutes % 60;
    let sec_rem = secs % 60;
    if hours > 0 {
        terminal::write(b"up ");
        terminal::write_num(hours);
        terminal::write(b":");
        if min_rem < 10 { terminal::write(b"0"); }
        terminal::write_num(min_rem);
        terminal::write(b":");
        if sec_rem < 10 { terminal::write(b"0"); }
        terminal::write_num(sec_rem);
    } else {
        terminal::write(b"up ");
        terminal::write_num(min_rem);
        terminal::write(b" min ");
        terminal::write_num(sec_rem);
        terminal::write(b" sec");
    }
    terminal::write(b", load average: 0.00, 0.00, 0.00\n");
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
    let mut pid: usize = 0;
    for &ch in args {
        if ch >= b'0' && ch <= b'9' {
            pid = pid * 10 + (ch - b'0') as usize;
        } else {
            break;
        }
    }
    if crate::syscall::kill_process(pid) {
        terminal::write(b"process ");
        terminal::write_num(pid as u64);
        terminal::write(b" killed (signal 9)\n");
    } else {
        terminal::write(b"kill: invalid PID or process not found\n");
    }
}

pub unsafe fn cmd_df(_args: &[u8]) {
    let (total, used) = crate::fs::data_pool_stats();
    let free = total.saturating_sub(used);
    terminal::write(b"Filesystem      Size    Used    Free  Mounted on\n");
    terminal::write(b"ramfs         ");
    pad_num(total as u64 / 1024, 7);
    terminal::write(b"  ");
    pad_num(used as u64 / 1024, 6);
    terminal::write(b"  ");
    pad_num(free as u64 / 1024, 6);
    terminal::write(b"  / (data pool)\n");
    terminal::write(b"Total: ");
    terminal::write_num(total as u64 / 1024);
    terminal::write(b" KiB, used ");
    terminal::write_num(used as u64 / 1024);
    terminal::write(b" KiB\n");
}

pub unsafe fn cmd_du(args: &[u8]) {
    let human = args.len() >= 3 && args[0] == b'-' && args[1] == b's' && args[2] == b'h';
    let path = if human {
        if args.len() <= 3 { &[] } else { &args[4..] }
    } else {
        args
    };
    
    let dir = if path.is_empty() {
        crate::fs::cwd()
    } else {
        match crate::fs::resolve(path) {
            Some(n) if crate::fs::kind(n) == crate::fs::Kind::Dir => n,
            Some(_) => { terminal::write(b"du: not a directory\n"); return; }
            None => { terminal::write(b"du: no such path\n"); return; }
        }
    };
    
    fn du_sum(node: u16) -> u32 {
        unsafe {
            let mut total = crate::fs::size_of(node);
            if crate::fs::kind(node) == crate::fs::Kind::Dir {
                crate::fs::for_each_child(node, |c| total += du_sum(c));
            }
            total
        }
    }
    
    let size = du_sum(dir);
    let name = crate::fs::node_name(dir);
    
    if human {
        let (unit, scaled) = if size >= 1024*1024 {
            (b"M", size / (1024*1024))
        } else if size >= 1024 {
            (b"K", size / 1024)
        } else {
            (b"B", size)
        };
        terminal::write_num(scaled as u64);
        terminal::write(unit);
        terminal::write(b"\t");
        terminal::write(name);
        terminal::write(b"\n");
    } else {
        terminal::write_num(size as u64);
        terminal::write(b"\t");
        terminal::write(name);
        terminal::write(b"\n");
    }
}

pub unsafe fn cmd_free(_args: &[u8]) {
    let s = crate::frame::real_stats();
    terminal::write(b"              total        used        free  (KiB, frame pool)\n");
    terminal::write(b"Mem:   ");
    pad_num(s.total_bytes / 1024, 12);
    pad_num(s.used_bytes / 1024, 12);
    pad_num(s.free_bytes / 1024, 12);
    terminal::write(b"\n");
    terminal::write(b"Swap:            0           0           0  (no swap: RAM/ROM only)\n");
}

/// meminfo — подробная сводка по памяти (frame-pool + топология).
pub unsafe fn cmd_meminfo() {
    let s = crate::frame::real_stats();
    terminal::write(b"=== PureOS Memory ===\n");
    terminal::write(b"Frame pool total: "); terminal::write_num(s.total_bytes / 1024); terminal::write(b" KiB (");
    terminal::write_num(s.total_frames); terminal::write(b" frames)\n");
    terminal::write(b"Frame pool used:  "); terminal::write_num(s.used_bytes / 1024); terminal::write(b" KiB (");
    terminal::write_num(s.used_frames); terminal::write(b" frames)\n");
    terminal::write(b"Frame pool free:  "); terminal::write_num(s.free_bytes / 1024); terminal::write(b" KiB (");
    terminal::write_num(s.free_frames); terminal::write(b" frames)\n");
    terminal::write(b"RAM base (hw):    "); terminal::write_hex(crate::hw::ram_base()); terminal::write(b"\n");
    terminal::write(b"Model: immutable ephemeral (no heap, bump layers, free-list reclamation)\n");
}

/// cpuinfo — сведения о процессоре прямо из CPUID.
pub unsafe fn cmd_cpuinfo() {
    let mut vendor = [0u8; 12];
    crate::hw::cpu_vendor(&mut vendor);
    let mut brand = [0u8; 48];
    let blen = crate::hw::cpu_brand(&mut brand);
    let f = crate::hw::cpu_features();

    terminal::write(b"vendor_id   : "); terminal::write(&vendor); terminal::write(b"\n");
    terminal::write(b"model name  : "); terminal::write(&brand[..blen]); terminal::write(b"\n");
    terminal::write(b"threads     : "); terminal::write_num(crate::hw::cpu_threads() as u64); terminal::write(b"\n");
    terminal::write(b"flags       :");
    if f.fpu { terminal::write(b" fpu"); }
    if f.tsc { terminal::write(b" tsc"); }
    if f.apic { terminal::write(b" apic"); }
    if f.sse { terminal::write(b" sse"); }
    if f.sse2 { terminal::write(b" sse2"); }
    if f.avx { terminal::write(b" avx"); }
    if f.aes { terminal::write(b" aes"); }
    if f.rdrand { terminal::write(b" rdrand"); }
    if crate::hw::has_rdseed() { terminal::write(b" rdseed"); }
    terminal::write(b"\n");
}

/// lspci — список PCI-устройств, отсканированных в железе.
pub unsafe fn cmd_lspci() {
    let devs = crate::hw::pci_devices();
    if devs.is_empty() {
        terminal::write(b"no PCI devices found\n");
        return;
    }
    for d in devs {
        // bus:slot.func
        write_hex2(d.bus); terminal::write(b":");
        write_hex2(d.slot); terminal::write(b".");
        terminal::write_num(d.func as u64);
        terminal::write(b"  ");
        terminal::write(crate::hw::pci_class_name(d.class, d.subclass));
        terminal::write(b" [");
        write_hex4(d.vendor); terminal::write(b":"); write_hex4(d.device);
        terminal::write(b"]\n");
    }
}

/// hwinfo — сводная информация об оборудовании.
pub unsafe fn cmd_hwinfo() {
    terminal::write(b"=== PureOS Hardware ===\n");
    cmd_cpuinfo();
    terminal::write(b"\n-- Memory --\n");
    let s = crate::frame::stats();
    let total = crate::frame::total_physical_memory();
    terminal::write(b"total RAM  : "); terminal::write_num(total / (1024 * 1024)); terminal::write(b" MiB\n");
    terminal::write(b"frame pool : "); terminal::write_num(s.total_bytes / (1024 * 1024)); terminal::write(b" MiB\n");
    terminal::write(b"used       : "); terminal::write_num(s.used_bytes / (1024 * 1024)); terminal::write(b" MiB\n");
    terminal::write(b"free       : "); terminal::write_num(s.free_bytes / (1024 * 1024)); terminal::write(b" MiB\n");
    terminal::write(b"\n-- PCI --\n");
    cmd_lspci();
}

// Вспомогательные форматтеры для hex-полей PCI.
unsafe fn write_hex2(v: u8) {
    let hex = b"0123456789abcdef";
    terminal::putchar(hex[(v >> 4) as usize]);
    terminal::putchar(hex[(v & 0xF) as usize]);
}
unsafe fn write_hex4(v: u16) {
    write_hex2((v >> 8) as u8);
    write_hex2((v & 0xFF) as u8);
}

/// Напечатать число, дополнив пробелами слева до ширины `width`.
unsafe fn pad_num(val: u64, width: usize) {
    let mut tmp = [0u8; 20];
    let mut i = tmp.len();
    let mut v = val;
    if v == 0 { i -= 1; tmp[i] = b'0'; }
    while v > 0 { i -= 1; tmp[i] = b'0' + (v % 10) as u8; v /= 10; }
    let digits = tmp.len() - i;
    for _ in digits..width { terminal::putchar(b' '); }
    terminal::write(&tmp[i..]);
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

pub unsafe fn cmd_imgview(args: &[u8]) {
    if args.is_empty() {
        terminal::write(b"usage: imgview <path>\n");
        terminal::write(b"Display a BMP or JPEG image file on screen.\n");
        return;
    }
    // Check extension
    let ext = {
        let dot = args.iter().rposition(|&c| c == b'.').map(|i| &args[i+1..]).unwrap_or(b"");
        if dot.len() >= 4 { &dot[..4] } else { dot }
    };
    if ext == b"bmp" || ext == b"BMP" {
        crate::image::display_bmp_file(args);
    } else if ext == b"jpg" || ext == b"JPG" || ext == b"jpeg" || ext == b"JPEG" {
        crate::jpeg::display_jpeg_file(args);
    } else if ext == b"gif" || ext == b"GIF" {
        crate::gif::display_gif_file(args);
    } else {
        // Auto-detect by trying decoders in order
        if !crate::image::display_bmp_file_full(args) {
            crate::jpeg::display_jpeg_file(args);
        }
    }
}

pub unsafe fn cmd_jpeg(args: &[u8]) {
    if args.is_empty() {
        terminal::write(b"usage: jpeg <path>\n");
        terminal::write(b"Display a JPEG image file on screen.\n");
        return;
    }
    crate::jpeg::display_jpeg_file(args);
}

pub unsafe fn cmd_gif(args: &[u8]) {
    if args.is_empty() {
        terminal::write(b"usage: gif <path>\n");
        terminal::write(b"Display an animated GIF image file on screen.\n");
        terminal::write(b"Press any key to exit the viewer.\n");
        return;
    }
    crate::gif::display_gif_file(args);
}

pub unsafe fn cmd_net(args: &[u8]) {
    if args.is_empty() || args == b"help" {
        terminal::write(b"net commands:\n");
        terminal::write(b"  net init      - init RTL8139 NIC\n");
        terminal::write(b"  net dhcp      - DHCP negotiation\n");
        terminal::write(b"  net dns <host>  - resolve hostname\n");
        terminal::write(b"  net http <url>  - HTTP GET (host/path)\n");
        terminal::write(b"  net status    - show network status\n");
        return;
    }

    let mut parts = args.split(|&c| c == b' ');
    let cmd = parts.next().unwrap_or(b"");

    match cmd {
        b"init" => {
            if crate::net::nic::init() {
                terminal::write(b"net: NIC ready\n");
            }
        }
        b"dhcp" => {
            // Initialize NIC first if needed
            let mut ipbuf = [0u8; 16];
            let nl = crate::net::fmt_ip(&crate::net::OUR_IP, &mut ipbuf);
            if nl == 3 && ipbuf[0] == b'0' && ipbuf[1] == b'.' && ipbuf[2] == b'0' {
                // Not configured — need to init first
                if !crate::net::nic::init() { return; }
            }
            crate::net::dhcp::dhcp_negotiate();
        }
        b"dns" => {
            let host = parts.next().unwrap_or(b"");
            if host.is_empty() {
                terminal::write(b"usage: net dns <hostname>\n");
                return;
            }
            let ip = crate::net::dns::dns_resolve(host);
            if ip == [0; 4] {
                terminal::write(b"dns: resolution failed\n");
            } else {
                terminal::write(host);
                terminal::write(b" -> ");
                let mut ipbuf = [0u8; 16];
                let nl = crate::net::fmt_ip(&ip, &mut ipbuf);
                terminal::write(&ipbuf[..nl]);
                terminal::write(b"\n");
            }
        }
        b"http" | b"get" => {
            let url = parts.next().unwrap_or(b"");
            if url.is_empty() {
                terminal::write(b"usage: net http <host>/<path>\n");
                return;
            }
            // Split host/path at first '/'
            let mut host = &url[..];
            let mut path: &[u8] = b"/";
            for i in 0..url.len() {
                if url[i] == b'/' {
                    host = &url[..i];
                    path = &url[i..];
                    break;
                }
            }
            let mut resp = [0u8; 4096];
            let n = crate::net::http::http_get(host, path, &mut resp);
            if n > 0 {
                // Print first part of response
                let show = n.min(1024);
                terminal::write(&resp[..show]);
                terminal::write(b"\n");
            }
        }
        b"status" => {
            terminal::write(b"Network status:\n");
            terminal::write(b"  MAC: ");
            for i in 0..6 {
                let hi = crate::net::OUR_MAC[i] >> 4;
                let lo = crate::net::OUR_MAC[i] & 0x0F;
                terminal::write(&[
                    if hi < 10 { b'0' + hi } else { b'a' + hi - 10 },
                    if lo < 10 { b'0' + lo } else { b'a' + lo - 10 },
                ]);
                if i < 5 { terminal::write(b":"); }
            }
            terminal::write(b"\n  IP: ");
            let mut ipbuf = [0u8; 16];
            let nl = crate::net::fmt_ip(&crate::net::OUR_IP, &mut ipbuf);
            terminal::write(&ipbuf[..nl]);
            terminal::write(b"\n  GW: ");
            let nl = crate::net::fmt_ip(&crate::net::GATEWAY, &mut ipbuf);
            terminal::write(&ipbuf[..nl]);
            terminal::write(b"\n  DNS: ");
            let nl = crate::net::fmt_ip(&crate::net::DNS_SERVER, &mut ipbuf);
            terminal::write(&ipbuf[..nl]);
            terminal::write(b"\n");
        }
        _ => {
            terminal::write(b"Unknown net command: ");
            terminal::write(cmd);
            terminal::write(b"\n");
        }
    }
}

pub unsafe fn cmd_cpu(args: &[u8]) {
    terminal::write(b"CPU Information:\n");
    terminal::write(b"  Count:  ");
    terminal::write_num(crate::smp::cpu_count() as u64);
    terminal::write(b"\n");
    terminal::write(b"  Features: syscall sse sse2 apic\n");
    terminal::write(b"  Mode:    64-bit long mode\n");
    terminal::write(b"  Kernel:  ring0, User: ring3 via SYSCALL/SYSRET\n\n");
    let _ = args;
}

pub unsafe fn cmd_pci(args: &[u8]) {
    cmd_lspci();
    let _ = args;
}

pub unsafe fn cmd_font(args: &[u8]) {
    // Build "font <args>" buffer and pass to cmd_config
    let mut buf = [0u8; 520];
    buf[..5].copy_from_slice(b"font ");
    let mut i = 5;
    let mut j = 0;
    while j < args.len() && i < buf.len() - 1 {
        buf[i] = args[j];
        i += 1;
        j += 1;
    }
    cmd_config(&buf[..i]);
}

pub unsafe fn cmd_wallpaper(args: &[u8]) {
    if args.is_empty() || args == b"list" {
        terminal::write(b"Wallpapers:\n");
        terminal::write(b"  solid     Solid color fill\n");
        terminal::write(b"  gradient  Vertical gradient\n");
        terminal::write(b"  stripes   Horizontal stripes\n");
        terminal::write(b"  checkers  Checkerboard pattern\n");
        terminal::write(b"  radial    Radial gradient\n");
        terminal::write(b"  waves     Sine waves\n");
        terminal::write(b"  grid      Grid pattern\n");
        terminal::write(b"  noise     Static noise\n");
        terminal::write(b"  load <path>  Load BMP or raw RGB from file\n");
        return;
    }
    if args.len() > 5 && &args[..5] == b"load " {
        let path = &args[5..];
        // Try BMP first
        if crate::image::display_bmp_file_full(path) {
            terminal::write(b"Wallpaper set from: ");
            terminal::write(path);
            terminal::write(b"\n");
            return;
        }
        // Try raw RGB (320x200, 19200 bytes)
        let node = match crate::fs::resolve(path) {
            Some(n) if crate::fs::kind(n) == crate::fs::Kind::File => n,
            _ => { terminal::write(b"wallpaper: file not found\n"); return; }
        };
        let data = crate::fs::read(node);
        // Render raw RGB data (BMP-style BGR, bottom-up)
        let w = crate::framebuffer::width().min(320);
        let h = crate::framebuffer::height().min(200);
        for y in 0..h {
            for x in 0..w {
                let src_off = ((199 - y) * 320 + x) as usize * 3;
                if src_off + 3 <= data.len() {
                    let b = data[src_off];
                    let g = data[src_off + 1];
                    let r = data[src_off + 2];
                    crate::framebuffer::put(x, y, crate::framebuffer::Rgb(r, g, b));
                }
            }
        }
        return;
    }
    // Try as built-in wallpaper name
    crate::wallpaper::set_wallpaper_by_name(args);
}

pub unsafe fn cmd_desktop(args: &[u8]) {
    let _ = args;
    crate::desktop::run();
}

pub unsafe fn cmd_theme(args: &[u8]) {
    if args.is_empty() || args == b"list" {
        terminal::write(b"Available themes:\n");
        let themes: [&[u8]; 10] = [
            b"dark", b"light", b"amber", b"green", b"blue",
            b"matrix", b"retro", b"hacker", b"terminal", b"tron",
        ];
        for t in &themes {
            terminal::write(b"  ");
            terminal::write(t);
            terminal::write(b"\n");
        }
        return;
    }
    // Convert theme name to colors and apply
    let colors = match args {
        b"dark"     => (0u8, 255u8, 255u8, 255u8, 0u8, 0u8),   // fg_hi fg_lo bg_hi bg_lo accent
        b"light"    => (0u8, 0u8, 0u8, 255u8, 255u8, 255u8),
        b"amber"    => (255u8, 191u8, 0u8, 0u8, 0u8, 0u8),
        b"green"    => (0u8, 255u8, 85u8, 0u8, 0u8, 0u8),
        b"blue"     => (79u8, 183u8, 255u8, 6u8, 11u8, 16u8),
        b"matrix"   => (0u8, 255u8, 64u8, 0u8, 16u8, 0u8),
        b"retro"    => (255u8, 170u8, 64u8, 16u8, 8u8, 0u8),
        b"hacker"   => (51u8, 255u8, 51u8, 0u8, 0u8, 0u8),
        b"terminal" => (184u8, 242u8, 146u8, 6u8, 11u8, 16u8),
        b"tron"     => (0u8, 200u8, 255u8, 0u8, 8u8, 16u8),
        _ => { terminal::write(b"Unknown theme: "); terminal::write(args); terminal::write(b"\n"); return; }
    };
    crate::config::set_terminal_colors(crate::config::TerminalColors {
        foreground_r: colors.0,
        foreground_g: colors.1,
        foreground_b: colors.2,
        background_r: colors.3,
        background_g: colors.4,
        background_b: colors.5,
    });
    terminal::init();
    terminal::write(b"Theme set to: ");
    terminal::write(args);
    terminal::write(b"\n");
}

pub unsafe fn cmd_ver() {
    terminal::write(b"PureOS Crystal v0.4.0\n");
    terminal::write(b"Immutable Ephemeral Kernel\n");
    terminal::write(b"Arch: x86_64 AMD64 | Firmware: UEFI\n");
    terminal::write(b"Zero-alloc | Preemptive RR | Rendezvous IPC\n");
    terminal::write(b"Built: 2026-07\n");
}

pub unsafe fn cmd_man(args: &[u8]) {
    crate::documentation::man(args);
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
            terminal::init();
            terminal::write(b"Configuration reset to defaults\n");
        }
        b"preset" => {
            crate::config::apply_preset(value);
            terminal::init();
            terminal::write(b"Preset applied: ");
            terminal::write(value);
            terminal::write(b"\n");
        }
        b"save" => {
            crate::config::save_config();
        }
        b"load" => {
            crate::config::load_config();
            terminal::init();
            terminal::write(b"Configuration loaded from /etc/pureos.conf\n");
        }
        b"font" => {
            // Попробовать распознать имя шрифта
            let names: [&[u8]; 8] = [b"compact", b"bold", b"italic", b"serif", b"outline", b"tall", b"vga", b"wide"];
            let mut found = None;
            for i in 0..names.len() {
                if value == names[i] {
                    crate::config::set_selected_font(i as u32);
                    crate::config::set_font_scale(1);
                    terminal::init();
                    terminal::write(b"Font set to ");
                    terminal::write(names[i]);
                    terminal::write(b"\n");
                    found = Some(());
                    break;
                }
            }
            if found.is_none() {
                // Если не имя — попробовать как число (scale)
                let scale = if value.is_empty() { 1u64 } else { parse_hex(value) };
                crate::config::set_font_scale(scale.clamp(1, 4) as u32);
                terminal::init();
                terminal::write(b"Font scale set to ");
                terminal::write_num(scale.clamp(1, 4));
                terminal::write(b"\n");
            }
        }
        b"prompt" => {
            crate::config::set_shell_prompt(value);
            terminal::write(b"Prompt set to: ");
            terminal::write(value);
            terminal::write(b"\n");
        }
        b"resolution" | b"res" => {
            if value.is_empty() {
                terminal::write(b"Current resolution: ");
                terminal::write_num(crate::framebuffer::width() as u64);
                terminal::write(b"x");
                terminal::write_num(crate::framebuffer::height() as u64);
                terminal::write(b"\n");
                terminal::write(b"Resolution change requires framebuffer re-init (not supported at runtime)\n");
            } else {
                terminal::write(b"Resolution change not supported at runtime\n");
            }
        }
        b"theme" | b"themes" => {
            let themes: [&[u8]; 10] = [
                b"dark", b"light", b"amber", b"green", b"blue",
                b"matrix", b"retro", b"hacker", b"terminal", b"tron",
            ];
            if value.is_empty() || value == b"list" {
                terminal::write(b"Available themes:\n");
                for t in &themes {
                    terminal::write(b"  ");
                    terminal::write(t);
                    terminal::write(b"\n");
                }
            } else {
                let mut found = false;
                for t in &themes {
                    if value == *t {
                        crate::config::apply_preset(if value == b"light" { b"light" } else { b"dark" });
                        terminal::write(b"Theme: ");
                        terminal::write(value);
                        terminal::write(b"\n");
                        found = true;
                        break;
                    }
                }
                if !found {
                    terminal::write(b"Unknown theme: ");
                    terminal::write(value);
                    terminal::write(b"\n");
                }
            }
        }
        _ => {
            terminal::write(b"usage: config [show|reset|preset|save|load|font|prompt|resolution|theme] [value]\n");
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

/// Glassmorphism: включить/выключить/установить уровень стеклянного эффекта.
/// glass [0|1|2|3] — 0=выкл, 1=лёгкий, 2=средний, 3=сильный
pub unsafe fn cmd_glass(args: &[u8]) {
    let level = if args.is_empty() {
        // toggle: если было 0 → 2, иначе → 0
        let current = crate::terminal::glass_mode();
        if current == 0 { 2 } else { 0 }
    } else {
        let trimmed = trim_ascii(args);
        if trimmed.len() == 1 && trimmed[0] >= b'0' && trimmed[0] <= b'3' {
            (trimmed[0] - b'0') as u32
        } else {
            terminal::write(b"Usage: glass [0|1|2|3]\n");
            terminal::write(b"  0 = off, 1 = light, 2 = medium, 3 = strong\n");
            return;
        }
    };

    crate::terminal::set_glass_mode(level);
    if level > 0 {
        terminal::write(b"Glassmorphism: ON (level ");
        terminal::write_num(level as u64);
        terminal::write(b")\n");
        terminal::write(b"Experience the frosted glass terminal.\n");
    } else {
        terminal::write(b"Glassmorphism: OFF\n");
    }
    // Перерисовать экран при включении
    if level > 0 || crate::terminal::glass_mode() == 0 {
        crate::terminal::clear();
    }
}

/// Rainbow: вывести текст радужными цветами.
pub unsafe fn cmd_rainbow(args: &[u8]) {
    if args.is_empty() {
        // Без аргументов — показать демо
        let msg = b"  PUREOS CRYSTAL KERNEL  ";
        let rainbow_colors = [
            crate::framebuffer::Rgb(255, 50, 50),    // red
            crate::framebuffer::Rgb(255, 165, 0),    // orange
            crate::framebuffer::Rgb(255, 255, 50),   // yellow
            crate::framebuffer::Rgb(50, 255, 50),    // green
            crate::framebuffer::Rgb(50, 200, 255),   // cyan
            crate::framebuffer::Rgb(100, 100, 255),  // blue
            crate::framebuffer::Rgb(200, 50, 255),   // purple
        ];
        for (i, &ch) in msg.iter().enumerate() {
            let color = rainbow_colors[i % rainbow_colors.len()];
            crate::terminal::set_colors(color, crate::terminal::current_bg());
            crate::terminal::putchar(ch);
        }
        crate::terminal::set_colors(
            crate::framebuffer::Rgb(255, 255, 255),
            crate::terminal::current_bg(),
        );
        crate::terminal::write(b"\n");
    } else {
        // Вывести аргументы радугой
        let rainbow_colors = [
            crate::framebuffer::Rgb(255, 50, 50),
            crate::framebuffer::Rgb(255, 165, 0),
            crate::framebuffer::Rgb(255, 255, 50),
            crate::framebuffer::Rgb(50, 255, 50),
            crate::framebuffer::Rgb(50, 200, 255),
            crate::framebuffer::Rgb(100, 100, 255),
            crate::framebuffer::Rgb(200, 50, 255),
        ];
        for (i, &ch) in args.iter().enumerate() {
            let color = rainbow_colors[i % rainbow_colors.len()];
            crate::terminal::set_colors(color, crate::terminal::current_bg());
            crate::terminal::putchar(ch);
        }
        crate::terminal::set_colors(
            crate::framebuffer::Rgb(255, 255, 255),
            crate::terminal::current_bg(),
        );
        crate::terminal::write(b"\n");
    }
}

// ═══════════════════════════════════════════════════════════════════
// Beep / Clock
// ═══════════════════════════════════════════════════════════════════

/// beep [freq] [ms] — пикнуть PC Speaker-ом.
pub unsafe fn cmd_beep(args: &[u8]) {
    let mut freq = 800u32;
    let mut dur = 100u32;
    let mut arg_idx = 0;
    let mut num = 0u32;
    let mut have_num = false;

    for &ch in args {
        if ch == b' ' || ch == b'\t' {
            if have_num {
                if arg_idx == 0 { freq = num.max(20).min(20000); }
                else if arg_idx == 1 { dur = num.min(5000); }
                arg_idx += 1;
                have_num = false;
                num = 0;
            }
            continue;
        }
        if ch >= b'0' && ch <= b'9' {
            num = num * 10 + (ch - b'0') as u32;
            have_num = true;
        }
    }
    if have_num {
        if arg_idx == 0 { freq = num.max(20).min(20000); }
        else if arg_idx == 1 { dur = num.min(5000); }
    }

    crate::terminal::write(b"Beep: ");
    crate::terminal::write_num(freq as u64);
    crate::terminal::write(b" Hz, ");
    crate::terminal::write_num(dur as u64);
    crate::terminal::write(b" ms\n");
    crate::pcspeaker::beep(freq, dur);
}

/// clock — показать текущее время из CMOS/RTC.
pub unsafe fn cmd_clock(_args: &[u8]) {
    let t = crate::cmos::read_rtc();
    let time_str = crate::cmos::format_time(t.hour, t.minute);
    crate::terminal::write(b"RTC time: ");
    crate::terminal::write(&time_str);
    crate::terminal::write(b"  ");
    crate::terminal::write_num(t.day as u64);
    crate::terminal::write(b".");
    crate::terminal::write_num(t.month as u64);
    crate::terminal::write(b".");
    crate::terminal::write_num(t.year as u64);
    crate::terminal::write(b"\n");
}

// ═══════════════════════════════════════════════════════════════════
// Sound commands
// ═══════════════════════════════════════════════════════════════════

/// play <note> <ms> [<note> <ms> ...] — сыграть мелодию.
pub unsafe fn cmd_play(args: &[u8]) {
    if args.is_empty() {
        // Демо-мелодия (C E G C6) — начало Оды к Радости
        crate::sound::play_freq(262, 150);  // C4
        crate::sound::play_freq(294, 150);  // D4
        crate::sound::play_freq(330, 150);  // E4
        crate::sound::play_freq(349, 150);  // F4
        crate::sound::play_freq(392, 150);  // G4
        crate::sound::play_freq(349, 150);  // F4
        crate::sound::play_freq(330, 150);  // E4
        crate::sound::play_freq(294, 150);  // D4
        crate::sound::play_freq(262, 300);  // C4
        crate::terminal::write(b"Played demo melody.\n");
        return;
    }
    // Parse args: alternating note/dur
    let mut tokens: [&[u8]; 32] = [b""; 32];
    let mut count = 0;
    let mut start = 0;
    for i in 0..=args.len() {
        if i == args.len() || args[i] == b' ' {
            if i > start {
                if count < 32 {
                    tokens[count] = &args[start..i];
                    count += 1;
                }
            }
            start = i + 1;
        }
    }
    crate::sound::play_melody(&tokens[..count]);
    crate::terminal::write(b"Played ");
    crate::terminal::write_num(count as u64);
    crate::terminal::write(b" notes.\n");
}

/// volume <0..100> — установить громкость.
pub unsafe fn cmd_volume(args: &[u8]) {
    if args.is_empty() {
        crate::terminal::write(b"Volume: ");
        crate::terminal::write_num(crate::sound::get_volume() as u64);
        crate::terminal::write(b"%\n");
        return;
    }
    let mut vol = 0u32;
    for &ch in args {
        if ch >= b'0' && ch <= b'9' {
            vol = vol * 10 + (ch - b'0') as u32;
        }
    }
    let vol = vol.min(100) as u8;
    crate::sound::set_volume(vol);
    crate::terminal::write(b"Volume set to ");
    crate::terminal::write_num(vol as u64);
    crate::terminal::write(b"%\n");
    crate::sound::confirm();
}

/// sound [on|off] — управление звуком.
pub unsafe fn cmd_sound(args: &[u8]) {
    if args.is_empty() {
        crate::terminal::write(b"Sound: ");
        if crate::sound::is_enabled() {
            crate::terminal::write(b"on, volume ");
            crate::terminal::write_num(crate::sound::get_volume() as u64);
            crate::terminal::write(b"%\n");
        } else {
            crate::terminal::write(b"off\n");
        }
        return;
    }
    if args.eq_ignore_ascii_case(b"on") {
        crate::sound::set_enabled(true);
        crate::sound::boot();
        crate::terminal::write(b"Sound on.\n");
    } else if args.eq_ignore_ascii_case(b"off") {
        crate::sound::set_enabled(false);
        crate::terminal::write(b"Sound off.\n");
    } else {
        crate::terminal::write(b"Usage: sound [on|off]\n");
    }
}

/// Обрезать пробельные символы (для парсинга аргументов).
fn trim_ascii(s: &[u8]) -> &[u8] {
    let mut start = 0;
    while start < s.len() && s[start] <= b' ' { start += 1; }
    let mut end = s.len();
    while end > start && s[end - 1] <= b' ' { end -= 1; }
    &s[start..end]
}
