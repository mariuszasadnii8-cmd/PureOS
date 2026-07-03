//! Man-страницы: Documentation system with sections (man 1/2/3/4/5/7).
//! Zero-Alloc: static data only.

use crate::terminal;

#[derive(Copy, Clone)]
pub struct ManPage {
    pub section: u8,
    pub name: &'static [u8],
    pub title: &'static [u8],
    pub content: &'static [u8],
    pub see_also: &'static [&'static [u8]],
}

const MAN1: &[ManPage] = &[
    ManPage {
        section: 1, name: b"ls", title: b"ls - list directory contents",
        content: b"SYNOPSIS\n  ls [-la] [path]\n\nDESCRIPTION\n  List contents of a directory.\n  Defaults to current directory if no path given.\n\nOPTIONS\n  -l    Long format: type, permissions, size, name\n  -a    Include all entries (no-op currently)\n\nEXAMPLES\n  ls\n  ls /home\n  ls -la /etc\n",
        see_also: &[b"cd", b"pwd", b"tree"],
    },
    ManPage {
        section: 1, name: b"cd", title: b"cd - change directory",
        content: b"SYNOPSIS\n  cd [path]\n\nDESCRIPTION\n  Change the current working directory.\n  With no argument or '~', goes to root '/'.\n  '..' goes up one level.\n\nEXAMPLES\n  cd /home/user\n  cd ..\n  cd\n",
        see_also: &[b"pwd", b"ls"],
    },
    ManPage {
        section: 1, name: b"mkdir", title: b"mkdir - make directory",
        content: b"SYNOPSIS\n  mkdir <path>\n\nDESCRIPTION\n  Create a new directory at the given path.\n  Parent directory must exist.\n\nEXAMPLES\n  mkdir mydir\n  mkdir /home/user/newdir\n",
        see_also: &[b"ls", b"touch", b"rm"],
    },
    ManPage {
        section: 1, name: b"touch", title: b"touch - create empty file",
        content: b"SYNOPSIS\n  touch <path>\n\nDESCRIPTION\n  Create an empty file at the given path.\n  If the file already exists, does nothing.\n",
        see_also: &[b"mkdir", b"write", b"cat"],
    },
    ManPage {
        section: 1, name: b"rm", title: b"rm - remove files",
        content: b"SYNOPSIS\n  rm [-rf] <path>\n\nDESCRIPTION\n  Remove a file or empty directory.\n\nOPTIONS\n  -r    Recursive: remove directory and all contents\n  -f    Force: suppress errors\n\nEXAMPLES\n  rm file.txt\n  rm -rf /tmp\n",
        see_also: &[b"mkdir", b"touch"],
    },
    ManPage {
        section: 1, name: b"cp", title: b"cp - copy files",
        content: b"SYNOPSIS\n  cp <source> <destination>\n\nDESCRIPTION\n  Copy a file from source to destination.\n  Destination can be a directory.\n",
        see_also: &[b"mv", b"ls"],
    },
    ManPage {
        section: 1, name: b"mv", title: b"mv - move/rename files",
        content: b"SYNOPSIS\n  mv <source> <destination>\n\nDESCRIPTION\n  Move or rename a file.\n  If destination is a directory, source is moved into it.\n",
        see_also: &[b"cp", b"ls"],
    },
    ManPage {
        section: 1, name: b"write", title: b"write - write text to file",
        content: b"SYNOPSIS\n  write <path> <text>\n\nDESCRIPTION\n  Write text to a file, overwriting existing content.\n  Creates the file if it doesn't exist.\n\nEXAMPLES\n  write /tmp/hello.txt Hello World\n",
        see_also: &[b"cat", b"touch"],
    },
    ManPage {
        section: 1, name: b"cat", title: b"cat - concatenate and display files",
        content: b"SYNOPSIS\n  cat <path>\n\nDESCRIPTION\n  Display the entire contents of a file.\n",
        see_also: &[b"head", b"tail", b"write"],
    },
    ManPage {
        section: 1, name: b"head", title: b"head - first lines of file",
        content: b"SYNOPSIS\n  head <path> [n]\n\nDESCRIPTION\n  Show the first n lines of a file.\n  Default: 10 lines.\n",
        see_also: &[b"tail", b"cat"],
    },
    ManPage {
        section: 1, name: b"tail", title: b"tail - last lines of file",
        content: b"SYNOPSIS\n  tail <path> [n]\n\nDESCRIPTION\n  Show the last n lines of a file.\n  Default: 10 lines.\n",
        see_also: &[b"head", b"cat"],
    },
    ManPage {
        section: 1, name: b"grep", title: b"grep - search file for pattern",
        content: b"SYNOPSIS\n  grep <pattern> <path>\n\nDESCRIPTION\n  Search a file for lines matching a pattern.\n  Prints matching lines.\n",
        see_also: &[b"cat", b"head"],
    },
    ManPage {
        section: 1, name: b"tree", title: b"tree - directory tree",
        content: b"SYNOPSIS\n  tree [path]\n\nDESCRIPTION\n  Show recursive directory structure as a tree.\n  Defaults to current directory.\n",
        see_also: &[b"ls", b"cd"],
    },
    ManPage {
        section: 1, name: b"stat", title: b"stat - file status",
        content: b"SYNOPSIS\n  stat <path>\n\nDESCRIPTION\n  Display metadata (name, type, size) for a file.\n",
        see_also: &[b"ls", b"du"],
    },
    ManPage {
        section: 1, name: b"du", title: b"du - directory/file size",
        content: b"SYNOPSIS\n  du [-sh] <path>\n\nDESCRIPTION\n  Show disk usage of a file or directory.\n\nOPTIONS\n  -s    Summary (total only)\n  -h    Human-readable sizes (B/K/M)\n",
        see_also: &[b"df", b"stat", b"ls"],
    },
    ManPage {
        section: 1, name: b"df", title: b"df - disk free",
        content: b"SYNOPSIS\n  df [-h]\n\nDESCRIPTION\n  Show filesystem data pool usage.\n\nOPTIONS\n  -h    Human-readable sizes\n",
        see_also: &[b"du", b"free"],
    },
    ManPage {
        section: 1, name: b"free", title: b"free - memory usage",
        content: b"SYNOPSIS\n  free\n\nDESCRIPTION\n  Show frame pool memory usage: total, used, free.\n",
        see_also: &[b"df", b"top"],
    },
    ManPage {
        section: 1, name: b"ps", title: b"ps - process list",
        content: b"SYNOPSIS\n  ps\n\nDESCRIPTION\n  List all processes with PID, state, and name.\n  States: RUN, SND, RCV, RPL, EXT.\n",
        see_also: &[b"kill", b"top"],
    },
    ManPage {
        section: 1, name: b"kill", title: b"kill - terminate a process",
        content: b"SYNOPSIS\n  kill <pid>\n\nDESCRIPTION\n  Send termination signal to a process.\n",
        see_also: &[b"ps", b"top"],
    },
    ManPage {
        section: 1, name: b"top", title: b"top - system monitor",
        content: b"SYNOPSIS\n  top\n\nDESCRIPTION\n  Interactive real-time system monitor.\n  Shows: uptime, memory, process table.\n  Keys: Q=quit.\n",
        see_also: &[b"ps", b"free", b"uptime"],
    },
    ManPage {
        section: 1, name: b"uptime", title: b"uptime - system uptime",
        content: b"SYNOPSIS\n  uptime\n\nDESCRIPTION\n  Show how long the system has been running.\n  Measured in APIC timer ticks (approx 100Hz).\n",
        see_also: &[b"top", b"info"],
    },
    ManPage {
        section: 1, name: b"hwinfo", title: b"hwinfo - hardware information",
        content: b"SYNOPSIS\n  hwinfo\n\nDESCRIPTION\n  Display CPU vendor, model, features, memory layout, and PCI devices.\n",
        see_also: &[b"info", b"free"],
    },
    ManPage {
        section: 1, name: b"info", title: b"info - detailed system info",
        content: b"SYNOPSIS\n  info\n\nDESCRIPTION\n  Show detailed system information including:\n  - kernel version and build\n  - scheduler and process counts\n  - memory usage\n  - disk status\n  - graphics subsystem\n",
        see_also: &[b"ver", b"hwinfo", b"top"],
    },
    ManPage {
        section: 1, name: b"ver", title: b"ver - kernel version",
        content: b"SYNOPSIS\n  ver\n\nDESCRIPTION\n  Display PureOS kernel version string.\n",
        see_also: &[b"info", b"uname"],
    },
    ManPage {
        section: 1, name: b"uname", title: b"uname - system identity",
        content: b"SYNOPSIS\n  uname [-a]\n\nDESCRIPTION\n  Show operating system name and architecture.\n\nOPTIONS\n  -a    All system information\n",
        see_also: &[b"ver", b"info"],
    },
    ManPage {
        section: 1, name: b"clear", title: b"clear - clear terminal",
        content: b"SYNOPSIS\n  clear\n\nDESCRIPTION\n  Clear the terminal screen and reset cursor position.\n",
        see_also: &[b"echo"],
    },
    ManPage {
        section: 1, name: b"echo", title: b"echo - print text",
        content: b"SYNOPSIS\n  echo <text>\n\nDESCRIPTION\n  Print text to the terminal.\n",
        see_also: &[b"clear"],
    },
    ManPage {
        section: 1, name: b"hex", title: b"hex - hex dump memory",
        content: b"SYNOPSIS\n  hex <address>\n\nDESCRIPTION\n  Dump 128 bytes of memory at the given address.\n  Address can be decimal or hex (0x prefix).\n",
        see_also: &[b"hwinfo"],
    },
    ManPage {
        section: 1, name: b"config", title: b"config - system configuration",
        content: b"SYNOPSIS\n  config [action] [value]\n\nDESCRIPTION\n  View or modify system configuration.\n\nACTIONS\n  show            Display current configuration\n  reset           Reset to defaults\n  preset <name>   Apply preset (default|minimal|performance)\n  save            Save to /etc/pureos.conf\n  load            Load from /etc/pureos.conf\n  font <name|n>   Set font (compact|bold|italic|serif|outline|tall|vga|wide) or scale (1-4)\n  prompt <text>   Set shell prompt\n",
        see_also: &[b"font", b"theme"],
    },
    ManPage {
        section: 1, name: b"barrel", title: b"barrel - Barrel REPL",
        content: b"SYNOPSIS\n  barrel\n\nDESCRIPTION\n  Enter the Barrel scripting language interactive REPL.\n  Type 'exit' to return to shell.\n  Supports: variables, arithmetic, if/else, while, loop.\n",
        see_also: &[b"cc", b"run", b"barrelc"],
    },
    ManPage {
        section: 1, name: b"cc", title: b"cc - compile Barrel to native",
        content: b"SYNOPSIS\n  cc <source_code>\n\nDESCRIPTION\n  Compile Barrel source code into a native ring3 ELF process.\n  One-pass x86_64 code generator.\n\nEXAMPLES\n  cc let x=7; let y=6; println x*y;\n",
        see_also: &[b"barrel", b"run", b"barrelc"],
    },
    ManPage {
        section: 1, name: b"run", title: b"run - execute Barrel script file",
        content: b"SYNOPSIS\n  run <path>\n\nDESCRIPTION\n  Load and execute a Barrel script from the filesystem.\n",
        see_also: &[b"barrel", b"cc"],
    },
    ManPage {
        section: 1, name: b"man", title: b"man - manual pages",
        content: b"SYNOPSIS\n  man [section] <topic>\n\nDESCRIPTION\n  Display the manual page for a topic.\n  With no arguments, show the index of all man pages.\n\nSECTIONS\n  1    User commands (shell)\n  2    System calls\n  3    Libraries (Barrel, barrelc, graphics)\n  4    System (kernel, architecture)\n  5    File formats\n  7    Miscellaneous\n\nEXAMPLES\n  man ls\n  man 1 ls\n  man 2 exec_elf\n  man barrel\n",
        see_also: &[b"help", b"info"],
    },
    ManPage {
        section: 1, name: b"help", title: b"help - command reference",
        content: b"SYNOPSIS\n  help\n\nDESCRIPTION\n  Display categorized command reference.\n  For detailed help on a specific command, use 'man <command>'.\n",
        see_also: &[b"man", b"info"],
    },
    ManPage {
        section: 1, name: b"imgview", title: b"imgview - display image",
        content: b"SYNOPSIS\n  imgview <path>\n\nDESCRIPTION\n  Display an image file on screen.\n  File extension determines decoder.\n  Supported formats:\n    .bmp  - 24-bit uncompressed\n    .jpeg/.jpg - baseline JPEG (SOF0, 4:4:4/4:2:2/4:2:0, 8-bit)\n    .gif  - GIF87a/89a (animated, LZW, transparency)\n\nEXAMPLES\n  imgview photo.bmp\n  imgview image.jpg\n  imgview animation.gif\n",
        see_also: &[b"jpeg", b"gif", b"wallpaper", b"bmp"],
    },
    ManPage {
        section: 1, name: b"gif", title: b"gif - display animated GIF",
        content: b"SYNOPSIS\n  gif <path>\n\nDESCRIPTION\n  Display an animated GIF image on screen.\n  Supports:\n    - GIF87a and GIF89a\n    - Global and local colour tables (up to 256 entries)\n    - Interlaced images\n    - Transparency\n    - Animation with frame delays (via APIC timer)\n    - Disposal methods 0-3\n  LZW decompression with 12-bit dictionary (4096 entries).\n  Press any key to exit the animated viewer.\n\nSEE ALSO\n  imgview, jpeg, wallpaper\n",
        see_also: &[b"imgview", b"jpeg"],
    },
    ManPage {
        section: 1, name: b"jpeg", title: b"jpeg|jpg - display JPEG image",
        content: b"SYNOPSIS\n  jpeg <path>\n  jpg <path>\n\nDESCRIPTION\n  Display a JPEG image file on screen.\n  Supports baseline JPEG (SOF0):\n    - YCbCr 4:4:4, 4:2:2, 4:2:0 subsampling\n    - 8-bit precision\n    - Single scan (non-progressive)\n    - All standard Huffman/quantization tables\n  Integer IDCT with 9-bit precision. Byte-stuffed streams handled.\n\nSEE ALSO\n  imgview, wallpaper\n",
        see_also: &[b"imgview"],
    },
    ManPage {
        section: 1, name: b"net", title: b"net - network control",
        content: b"SYNOPSIS\n  net init [ioport]       Initialize RTL8139 NIC at given I/O port\n  net dhcp                Run DHCP client to obtain IP configuration\n  net dns <hostname>      Resolve DNS name to IP address\n  net http <url>          HTTP GET a URL (e.g. http://example.com/)\n  net status              Show network status (MAC, IP, gateway, DNS)\n\nDESCRIPTION\n  Network subsystem control command.\n  The NIC driver supports RTL8139 PCI Ethernet.\n\nSUBCOMMANDS\n  init      Detect and initialize RTL8139 at specified I/O port\n            (default: 0x300 if not specified)\n  dhcp      Broadcast DHCP discover, listen for offer/ack.\n            Sets IP, gateway, netmask, and DNS server.\n  dns       Send DNS A-record query to the configured DNS server.\n            Example: net dns google.com\n  http      Issue HTTP GET request. URL must include host and path.\n            Example: net http http://example.com/index.html\n  status    Display NIC state: MAC address, IP, gateway, DNS.\n\nPROTOCOL STACK\n  RTL8139  PCI NIC driver (polling mode, no IRQ)\n  Ethernet II / ARP / IPv4 / UDP / TCP\n  DHCP client (broadcast, 4-message handshake)\n  DNS resolver (A records, UDP)\n  HTTP client (TCP: 3-way handshake, GET, response)\n\nLIMITATIONS\n  - RTL8139 only (will auto-detect via PCI vendor/device)\n  - Polling mode (no interrupt-driven RX)\n  - No ARP cache expiry\n  - TCP: single connection, no congestion control, no retransmit\n  - DNS: no cached answers (re-queries each time)\n  - HTTP: simple GET only, no chunked encoding\n\nSEE ALSO\n  hwinfo (PCI bus scan)\n",
        see_also: &[b"hwinfo", b"info"],
    },
    ManPage {
        section: 1, name: b"wallpaper", title: b"wallpaper - wallpaper manager",
        content: b"SYNOPSIS\n  wallpaper <name>\n  wallpaper list\n  wallpaper load <path>\n\nDESCRIPTION\n  Set or manage desktop wallpaper.\n  Built-in: solid, gradient, stripes, checkers, radial, waves, grid, noise.\n  Custom: load a BMP/RAW file from filesystem.\n",
        see_also: &[b"imgview", b"config"],
    },
    ManPage {
        section: 1, name: b"theme", title: b"theme - theme manager",
        content: b"SYNOPSIS\n  theme [name]\n  theme list\n\nDESCRIPTION\n  Set or list terminal color themes.\n  Built-in themes: dark, light, amber, green, blue, matrix,\n  retro, hacker, terminal, tron.\n\nEXAMPLES\n  theme matrix\n  theme list\n",
        see_also: &[b"config", b"wallpaper"],
    },
    ManPage {
        section: 1, name: b"reboot", title: b"reboot - reboot system",
        content: b"SYNOPSIS\n  reboot\n\nDESCRIPTION\n  Reboot the computer via UEFI Runtime Services.\n  Note: may cause #GP if firmware segment selectors are incompatible.\n",
        see_also: &[b"shutdown"],
    },
    ManPage {
        section: 1, name: b"shutdown", title: b"shutdown - power off",
        content: b"SYNOPSIS\n  shutdown\n\nDESCRIPTION\n  Power off the computer via UEFI Runtime Services.\n  Same firmware risk as reboot.\n",
        see_also: &[b"reboot"],
    },
    ManPage {
        section: 1, name: b"font", title: b"font - font selection",
        content: b"SYNOPSIS\n  font <name>\n  font list\n  font scale <n>\n\nDESCRIPTION\n  Select or list available fonts.\n  Fonts: compact, bold, italic, serif, outline, tall, vga, wide.\n\nEXAMPLES\n  font vga\n  font list\n  font scale 2\n",
        see_also: &[b"config", b"theme"],
    },
    ManPage {
        section: 1, name: b"history", title: b"history - command history",
        content: b"SYNOPSIS\n  history\n\nDESCRIPTION\n  Show command history (last 16 commands).\n",
        see_also: &[b"help"],
    },
    ManPage {
        section: 1, name: b"alias", title: b"alias - command aliases",
        content: b"SYNOPSIS\n  alias [name=value]\n  alias list\n\nDESCRIPTION\n  Create or list shell command aliases.\n  Without arguments, lists all aliases.\n\nEXAMPLES\n  alias ll=ls -la\n  alias list\n",
        see_also: &[b"help"],
    },
    ManPage {
        section: 1, name: b"snake", title: b"snake - snake game",
        content: b"SYNOPSIS\n  snake\n\nDESCRIPTION\n  Play the classic Snake game.\n  Controls: WASD to move, Q to quit.\n",
        see_also: &[b"demo"],
    },
    ManPage {
        section: 1, name: b"demo", title: b"demo - demo user process",
        content: b"SYNOPSIS\n  demo\n\nDESCRIPTION\n  Spawn a test user-space ring3 process.\n  Serves as a smoke test for the syscall bridge.\n",
        see_also: &[b"cc", b"snake"],
    },
    ManPage {
        section: 1, name: b"test", title: b"test - kernel tests",
        content: b"SYNOPSIS\n  test\n\nDESCRIPTION\n  Run the built-in kernel test suite.\n",
        see_also: &[b"demo"],
    },
    ManPage {
        section: 1, name: b"pwd", title: b"pwd - print working directory",
        content: b"SYNOPSIS\n  pwd\n\nDESCRIPTION\n  Show the current working directory path.\n",
        see_also: &[b"cd", b"ls"],
    },
    ManPage {
        section: 1, name: b"whoami", title: b"whoami - current user",
        content: b"SYNOPSIS\n  whoami\n\nDESCRIPTION\n  Show the current user name (always 'root').\n",
        see_also: &[b"id"],
    },
    ManPage {
        section: 1, name: b"id", title: b"id - user identity",
        content: b"SYNOPSIS\n  id\n\nDESCRIPTION\n  Show user and group IDs.\n",
        see_also: &[b"whoami"],
    },
];

const MAN2: &[ManPage] = &[
    ManPage {
        section: 2, name: b"syscalls", title: b"syscalls - system call reference",
        content: b"SYNOPSIS\n  syscall <number> <args...>\n\nDESCRIPTION\n  System calls are invoked via the SYSCALL instruction.\n  RAX = syscall number, args in RDI, RSI, RDX, R10, R8, R9.\n  Return value in RAX (negative = error).\n\nNUMBERS\n  1   memory_allocate(size, flags)\n  2   memory_free(addr)\n  3   create_process(entry)\n  4   create_thread(entry, stack)\n  5   yield_cpu()\n  6   exit_process(code)\n  7   send_ipc(target, msg_ptr)\n  8   receive_ipc(msg_ptr)\n  9   reply_ipc(target, msg_ptr)\n  15  exec_elf(ptr, size)\n  16  write(fd, buf, len)\n  17  read(fd, buf, len)\n  18  open(path, flags)\n  19  close(fd)\n  20  lseek(fd, off, whence)\n  21  stat(path, buf)\n  22  dup(fd)\n  23  fcntl(fd, cmd, arg)\n\nMAGIC (24-31):\n  24  print(msg)\n  25  println(msg)\n  26  input(buf, len)\n  27  ticks()\n  28  cls()\n  29  set_cursor(row, col)\n  30  color(fg, bg)\n  31  reboot()\n  32  print_num(value, newline)\n\nGRAPHICS (33-40):\n  33  get_screen_info(buf)\n  34  draw_pixel(x, y, color)\n  35  draw_line(x1, y1, x2, y2, color)\n  36  draw_rect(x, y, w, h, color, fill)\n  37  draw_circle(x, y, r, color, fill)\n  38  draw_image(buf, x, y, w, h)\n  39  clear_screen(color)\n  40  set_font_scale(scale)\n\nERROR CODES\n  -1  Invalid syscall number\n  -2  Invalid process ID\n  -3  No capacity (too many processes / FD table full)\n  -4  Invalid pointer\n  -5  Out of memory\n  -38 Unsupported operation\n",
        see_also: &[b"exec_elf", b"barrelc", b"graphics"],
    },
    ManPage {
        section: 2, name: b"exec_elf", title: b"exec_elf - execute ELF binary",
        content: b"SYNOPSIS\n  exec_elf(data_ptr, size) -> pid\n\nDESCRIPTION\n  Load a static PIE ELF64 binary as a new ring3 process.\n  Segments PT_LOAD are mapped with proper permissions.\n  Stack (64 KiB) is allocated after code.\n  Returns PID on success, negative on error.\n\nSEE ALSO\n  elf(5) for ELF format details.\n",
        see_also: &[b"syscalls", b"cc", b"barrelc"],
    },
    ManPage {
        section: 2, name: b"open", title: b"open - open file",
        content: b"SYNOPSIS\n  open(path, flags) -> fd\n\nDESCRIPTION\n  Open a file and return a file descriptor.\n  Returns negative on error.\n  Flags: 0=read, 1=write.\n",
        see_also: &[b"close", b"read", b"write", b"syscalls"],
    },
    ManPage {
        section: 2, name: b"read", title: b"read - read from file",
        content: b"SYNOPSIS\n  read(fd, buf, len) -> bytes_read\n\nDESCRIPTION\n  Read up to len bytes from a file descriptor.\n  Returns number of bytes read, or negative on error.\n  fd=0 reads from keyboard (blocking).\n",
        see_also: &[b"write", b"open", b"close", b"syscalls"],
    },
    ManPage {
        section: 2, name: b"write", title: b"write - write to file",
        content: b"SYNOPSIS\n  write(fd, buf, len) -> bytes_written\n\nDESCRIPTION\n  Write data to a file descriptor.\n  fd=1 writes to terminal.\n",
        see_also: &[b"read", b"open", b"syscalls"],
    },
    ManPage {
        section: 2, name: b"graphics", title: b"graphics - graphics syscalls",
        content: b"SYNOPSIS\n  Graphics subsystem syscalls (33-40).\n\nDESCRIPTION\n  33: get_screen_info(buf) - fill struct {w, h, stride, fmt}\n  34: draw_pixel(x, y, color) - set pixel\n  35: draw_line(x1, y1, x2, y2, color) - Bresenham\n  36: draw_rect(x, y, w, h, color, fill) - rect\n  37: draw_circle(x, y, r, color, fill) - midpoint\n  38: draw_image(buf, x, y, w, h) - blit RGB24\n  39: clear_screen(color) - fill with color\n  40: set_font_scale(scale) - 1 or 2\n\nCOLOR FORMAT\n  24-bit RGB: 0x00RRGGBB\n",
        see_also: &[b"syscalls", b"barrel"],
    },
    ManPage {
        section: 2, name: b"ipc", title: b"ipc - inter-process communication",
        content: b"SYNOPSIS\n  send_ipc(target, msg_ptr)\n  receive_ipc(msg_ptr)\n  reply_ipc(target, msg_ptr)\n\nDESCRIPTION\n  Synchronous rendezvous IPC.\n  No buffering in kernel - direct copy between address spaces.\n  Message size: 64 bytes.\n  All three operations block until the partner matches.\n",
        see_also: &[b"syscalls", b"process"],
    },
];

const MAN3: &[ManPage] = &[
    ManPage {
        section: 3, name: b"barrel", title: b"barrel - Barrel scripting language",
        content: b"SYNOPSIS\n  barrel (REPL) or run <script>\n\nDESCRIPTION\n  Barrel is a lightweight scripting language built into the kernel.\n  Zero-alloc interpreter with static buffers.\n\nSTATEMENTS\n  let name = expr;       Variable assignment\n  print expr;            Print value\n  println expr;          Print value with newline\n  input name;            Read string from keyboard\n  if cond { ... } else { ... }  Conditional\n  while cond { ... }     Loop\n  loop { ... }           Infinite loop\n  break;                 Exit loop\n  exit;                  Return from REPL\n\nEXPRESSIONS\n  + - * /                 Arithmetic\n  == != < > <= >=         Comparison\n  (expr)                  Grouping\n  identifier              Variable lookup\n\nEXAMPLES\n  let x = 42;\n  println x * 2;\n  if x > 10 { println \"big\"; }\n",
        see_also: &[b"barrelc", b"cc", b"bmp"],
    },
    ManPage {
        section: 3, name: b"barrelc", title: b"barrelc - Barrel compiler",
        content: b"SYNOPSIS\n  cc <source>\n\nDESCRIPTION\n  One-pass compiler from Barrel source to native x86_64 ELF64.\n  Generates position-independent code with rel32 jumps.\n  Uses syscall 32 (print_num) for output.\n\nPHASES\n  1. Lexer: tokenize source into tokens\n  2. Codegen: emit x86_64 machine code\n  3. ELF builder: wrap in minimal ELF64\n  4. elf::exec: load and run as ring3 process\n\nSUPPORTED\n  Variables (let)\n  Arithmetic (+, -, *, /)\n  Comparison (==, !=, <, >, <=, >=)\n  print/println\n  if/else, while, loop, break\n",
        see_also: &[b"barrel", b"cc", b"exec_elf"],
    },
    ManPage {
        section: 3, name: b"bmp", title: b"bmp - BMP image library",
        content: b"SYNOPSIS\n  imgview <file>\n\nDESCRIPTION\n  Built-in BMP image decoder.\n  Supports: BMP v3, 24-bit uncompressed (BI_RGB).\n  Max dimensions: 1920x1080.\n  Displayed directly on framebuffer.\n\nFORMAT\n  BMP file structure:\n  - BITMAPFILEHEADER (14 bytes)\n  - BITMAPINFOHEADER (40 bytes)  \n  - Pixel data (BGR, bottom-up)\n  - Row padding to 4-byte boundary\n",
        see_also: &[b"imgview", b"wallpaper", b"barrel"],
    },
    ManPage {
        section: 3, name: b"theme", title: b"theme - theme system",
        content: b"SYNOPSIS\n  theme [name]\n  theme list\n\nDESCRIPTION\n  Terminal color theme system.\n  10 built-in themes: Dark, Light, Amber, Green, Blue,\n  Matrix, Retro, Hacker, Terminal, Tron.\n  Themes set foreground, background, accent, selection colors.\n",
        see_also: &[b"config", b"wallpaper", b"font"],
    },
    ManPage {
        section: 3, name: b"font", title: b"font - font system",
        content: b"SYNOPSIS\n  font <name>\n  font list\n\nDESCRIPTION\n  8 built-in bitmap fonts:\n  compact   8x8  Default monospace\n  bold      8x8  Bold weight\n  italic    8x8  Italic (sheared)\n  serif     8x8  With serifs\n  outline   8x8  Hollow outline\n  tall      8x14 Taller characters\n  vga       8x16 Classic VGA font\n  wide      10x18 Wide characters\n\n  All fonts are embedded as bitmap data.\n  Scalable 1x-4x via font_scale config.\n",
        see_also: &[b"config", b"theme"],
    },
];

const MAN4: &[ManPage] = &[
    ManPage {
        section: 4, name: b"architecture", title: b"architecture - kernel architecture",
        content: b"DESCRIPTION\n  PureOS Crystal Kernel - Immutable Ephemeral Architecture.\n\nPRINCIPLES\n  1. Zero-Alloc: no heap in kernel, only static arrays\n  2. Ephemeral Layers: per-process memory that vanishes on exit\n  3. Crystal Topology: hardware frozen at boot, never changes\n  4. Preemptive Scheduling: APIC timer at ~100Hz\n  5. Identity-mapped physical memory\n\nCOMPONENTS\n  kernel/src/main.rs     Entry point, boot sequence\n  kernel/src/cpu.rs      GDT/TSS, segment management\n  kernel/src/idt.rs      Interrupt descriptor table\n  kernel/src/syscall.rs  Syscall handlers\n  kernel/src/apic.rs     Local APIC timer\n  kernel/src/frame.rs    Physical frame allocator\n  kernel/src/fs.rs       In-RAM filesystem\n  kernel/src/elf.rs      ELF loader\n  kernel/src/barrel.rs   Barrel interpreter\n  kernel/src/barrelc.rs  Barrel compiler\n",
        see_also: &[b"process", b"memory", b"v04"],
    },
    ManPage {
        section: 4, name: b"process", title: b"process - process model",
        content: b"DESCRIPTION\n  PureOS supports up to 64 processes.\n\nSTATES\n  Empty       Slot available\n  Runnable    Ready to execute\n  BlkSend     Blocked on send_ipc\n  BlkReceive  Blocked on receive_ipc\n  BlkReply    Blocked on reply_ipc\n  Exited      Terminated, waiting for collection\n\nSCHEDULING\n  Preemptive round-robin via APIC timer.\n  Timer interrupt at vector 0x20, ~100Hz.\n  Process 0 (shell) is never preempted.\n\nMEMORY\n  16KB kernel stack per process\n  64KB user stack per process\n  16MB ephemeral layer per process\n  Private PML4 per process\n",
        see_also: &[b"architecture", b"ipc", b"memory"],
    },
    ManPage {
        section: 4, name: b"memory", title: b"memory - memory management",
        content: b"DESCRIPTION\n  PureOS memory management.\n\nFRAME ALLOCATOR\n  Static pool of 4KB frames from UEFI.\n  Bump allocation for speed.\n  Free-list for reclamation.\n  Per-process frame tracking via linked list.\n  All frames freed on process exit.\n\nADDRESS SPACE\n  Identity-mapped physical memory (0-4GB).\n  Kernel mapped identically in all processes.\n  Ephemeral layer at 16 TiB (EPHEMERAL_BASE).\n  Private PML4 per process.\n  TLB flush on context switch (CR3 reload).\n",
        see_also: &[b"architecture", b"process"],
    },
    ManPage {
        section: 4, name: b"v04", title: b"v04 - version 0.4 changelog",
        content: b"DESCRIPTION\n  PureOS v0.4 - Major Improvements\n\nFEATURES\n  Frame reclamation (free-list + per-process tracking)\n  Private PML4 per process\n  APIC timer preemptive scheduling\n  ATA PIO disk driver + block filesystem\n  Per-process FD tables (open/close/read/write/lseek/stat)\n  Userspace C syscall wrappers (40 syscalls)\n  8-font system with algorithmic variants\n  10 built-in color themes\n  8 procedural wallpapers\n  Shell aliases\n  SMP foundation\n  IRQ1 keyboard interrupt mode\n  Man documentation sections 1/2/3/4/5/7\n  BMP image viewer\n  Process priorities\n",
        see_also: &[b"architecture", b"process"],
    },
];

const MAN5: &[ManPage] = &[
    ManPage {
        section: 5, name: b"elf", title: b"elf - ELF file format",
        content: b"DESCRIPTION\n  PureOS loads static PIE ELF64 binaries.\n\nREQUIREMENTS\n  - ELF64 (64-bit)\n  - ET_DYN (PIE) or ET_EXEC type\n  - x86_64 machine\n  - Static: no dynamic linking\n  - PT_LOAD segments only\n  - No relocations needed (position-independent)\n\nLOADING\n  1. elf::exec reads ELF header\n  2. Validates magic, class, machine, type\n  3. Maps PT_LOAD segments with proper permissions\n  4. Allocates 64KiB stack after code\n  5. Creates ring3 process with private PML4\n  6. Jumps to e_entry\n\nSEE ALSO\n  exec_elf(2), cc(1)\n",
        see_also: &[b"exec_elf", b"barrelc"],
    },
    ManPage {
        section: 5, name: b"bmp", title: b"bmp - BMP image format",
        content: b"DESCRIPTION\n  BMP (bitmap) image file format.\n  PureOS supports BMP v3, 24-bit uncompressed.\n\nHEADER (BITMAPFILEHEADER, 14 bytes)\n  u16 bfType = 0x4D42 ('BM')\n  u32 bfSize (file size)\n  u16 reserved1, reserved2\n  u32 bfOffBits (offset to pixel data)\n\nINFO HEADER (BITMAPINFOHEADER, 40 bytes)\n  u32 biSize = 40\n  i32 biWidth, biHeight\n  u16 biPlanes = 1\n  u16 biBitCount = 24\n  u32 biCompression = 0 (BI_RGB)\n  ...\n\nPIXEL DATA\n  BGR format (Blue-Green-Red triples)\n  Bottom-up row order\n  Each row padded to 4-byte align\n",
        see_also: &[b"imgview", b"wallpaper"],
    },
];

const MAN7: &[ManPage] = &[
    ManPage {
        section: 7, name: b"installation", title: b"installation - PureOS installation",
        content: b"SYNOPSIS\n  install\n\nDESCRIPTION\n  PureOS Installation Guide.\n\nREQUIREMENTS\n  - UEFI 2.0+ firmware\n  - x86_64 AMD64 processor\n  - 512MB RAM minimum\n  - 8GB disk space\n\nSTEPS\n  1. Boot PureOS installer\n  2. Select target disk\n  3. Choose partition scheme\n  4. Create EFI System Partition (512MB, FAT32)\n  5. Install bootloader to ESP\n  6. Copy kernel to \\EFI\\PUREOS\\KERNEL.ELF\n  7. Save config and reboot\n",
        see_also: &[b"architecture", b"v04"],
    },
    ManPage {
        section: 7, name: b"ata", title: b"ata - ATA PIO driver",
        content: b"SYNOPSIS\n  hwinfo (shows disk status)\n\nDESCRIPTION\n  ATA PIO (Programmed I/O) disk driver.\n\nREGISTERS (Primary Channel)\n  0x1F0  Data port (16-bit)\n  0x1F1  Error/Features\n  0x1F2  Sector count\n  0x1F3-5 LBA 24-47\n  0x1F6  Drive/head\n  0x1F7  Status/Command\n  0x3F6  Control\n\nCOMMANDS\n  0xEC  IDENTIFY - detect disk, read model\n  0x20  READ SECTORS (LBA28)\n  0x30  WRITE SECTORS (LBA28)\n\nSEE ALSO\n  blockfs(7)\n",
        see_also: &[b"blockfs", b"hwinfo"],
    },
    ManPage {
        section: 7, name: b"blockfs", title: b"blockfs - block filesystem",
        content: b"DESCRIPTION\n  Persistent block filesystem layered on ATA PIO.\n\nSUPERBLOCK\n  Magic: 0x50555245 ('PURE')\n  Version, block count, inode count\n\nINODES\n  64 inodes, 32 bytes each\n  Type, size, direct block pointers\n  Free bitmap\n\nBLOCKS\n  4KB data blocks\n  64 blocks, 256KB total\n  Free bitmap\n\nSEE ALSO\n  ata(7)\n",
        see_also: &[b"ata", b"fs"],
    },
    ManPage {
        section: 7, name: b"smp", title: b"smp - symmetric multiprocessing",
        content: b"DESCRIPTION\n  SMP subsystem for PureOS.\n\nSTATUS\n  CPU count detected via CPUID (hw::cpu_threads).\n  AP wakeup via INIT-SIPI-SIPI protocol (vector 0x08).\n  AP trampoline at physical 0x8000 (hand-coded 16/32/64-bit transition).\n  Per-CPU data array (smp::PerCpu, MAX_CPUS=8).\n  Per-CPU kernel stacks (16 KiB each).\n  APs enter idle loop (pause + hlt), process IPI work queue.\n  IPI via LAPIC ICR (vector 0x21).\n  Work queue: static array, lock-free single-consumer.\n  BSP switches to PERCPU_ARRAY[0] as GS base.\n\nCURRENT LIMITATIONS\n  APIC ID assumed = cpu_id (no ACPI MADT parsing).\n  QEMU only tested (-smp N).\n  Per-CPU TSS not set up on APs (ist stacks use BSP).\n  All user processes run on BSP only.\n  Work queue has no wake-up mechanism (AP polls via hlt).\n\nFUTURE\n  ACPI MADT parsing for real APIC ID discovery.\n  Per-CPU TSS + IST stacks.\n  Per-CPU run queues for load-balanced scheduling.\n  Cross-CPU wakeup (IPI + scheduler).\n  Cache-coherent memory awareness.\n",
        see_also: &[b"architecture", b"apic", b"process"],
    },
    ManPage {
        section: 7, name: b"apic", title: b"apic - APIC and IPI",
        content: b"DESCRIPTION\n  Local APIC: timer for preemptive scheduling + IPI for SMP.\n\nTIMER CONFIGURATION\n  MMIO base: 0xFEE00000\n  Divide by 1, Periodic mode, Vector 0x20\n  Initial count: 10,000,000 (~100Hz on 1GHz CPU)\n\nIPI FUNCTIONS\n  send_ipi(apic_id, vector)      Fixed IPI to specific APIC ID\n  send_init_ipi(apic_id)         INIT IPI for AP wakeup\n  send_startup_ipi(apic_id, vec) SIPI (AP starts at vec*0x1000)\n  send_ipi_all_others(vector)    Broadcast to all except self\n\nFLOW\n  1. APIC timer fires -> IDT vector 0x20\n  2. timer_stub saves regs, calls timer_handler\n  3. timer_handler -> timer_tick() -> eoi()\n  4. timer_tick_handler increments TICK_COUNT\n  5. Every ~5 ticks, context_switch to next process\n  6. Process 0 (shell) not preempted\n\nIPI FLOW\n  1. send_ipi writes ICR_HIGH (APIC ID) + ICR_LOW (vector)\n  2. Receiving CPU's LAPIC delivers interrupt at vector\n  3. IDT entry -> ipi_stub -> ipi_handler -> process_work_queue\n",
        see_also: &[b"process", b"smp"],
    },
    ManPage {
        section: 7, name: b"fs", title: b"fs - ram filesystem",
        content: b"DESCRIPTION\n  In-RAM filesystem (ramfs).\n  Volatile: disappears on reboot.\n\nLIMITS\n  256 max nodes\n  28 chars max name length\n  256KB data pool\n  O(n) child lookup (linear scan)\n  Bump allocation for data\n  No garbage collection on delete\n\nSTRUCTURE\n  Root (/) created at boot.\n  Default dirs: /bin, /dev, /etc, /home, /tmp\n  /etc/motd, /etc/release, /etc/pureos-version pre-created.\n\nSEE ALSO\n  blockfs(7) for persistent storage\n",
        see_also: &[b"blockfs", b"ls"],
    },
    ManPage {
        section: 7, name: b"usb", title: b"usb - USB subsystem",
        content: b"SYNOPSIS\n  usb [list|test|scan]\n  mouse\n\nDESCRIPTION\n  USB host controller driver (EHCI only).\n  Polling mode, no IRQ.\n  Supports HID boot protocol keyboards and mice.\n\nSUBCOMMANDS\n  list      Show detected USB devices with details\n  test      Interactive keyboard test (ESC to exit)\n  scan      Force re-enumeration of USB bus\n  mouse     Show mouse cursor position and button state\n\nEXAMPLES\n  usb list            List all USB devices\n  usb test            Test USB keyboard input\n  mouse               Show mouse state\n\nSUBSUBSYSTEMS\n  EHCI    Enhanced Host Controller Interface (USB 2.0)\n  HID     Human Interface Device (keyboard + mouse boot protocol)\n\nARCHITECTURE\n  PCI scan for class 0x0C/0x03/0x20 (USB 2.0 EHCI).\n  MMIO registers: CAPLENGTH, HCSPARAMS, HCCPARAMS, operational regs.\n  Async schedule for control transfers (enumeration).\n  Periodic schedule for interrupt transfers (HID reports).\n  Two periodic QHs: keyboard -> mouse (linked list).\n  Static QH/qTD pools (no heap allocation).\n  Root hub polling: port connect, reset, enable.\n  Device enumeration: GET_DESCRIPTOR -> SET_ADDRESS -> SET_CONFIGURATION.\n  HID subclass detection: 1 = keyboard, 2 = mouse.\n  HID setup: SET_PROTOCOL (boot), SET_IDLE.\n  Interrupt polling: 8-byte keyboard + 3-byte mouse reports.\n  Keycode-to-ASCII: US layout, shift support.\n  Mouse cursor: 12x16 arrow bitmap, save/restore background.\n\nLIMITATIONS\n  EHCI only (no OHCI/UHCI/XHCI).\n  Polling mode only (no IRQ sharing).\n  HID boot protocol only (no full report descriptor parsing).\n  US keyboard layout only.\n  Mouse: relative mode, no wheel, no acceleration.\n  No USB hubs (direct device connect).\n  No isochronous transfers.\n  No USB mass storage.\n\nSEE ALSO\n  pci(1), hwinfo(1), architecture(4), mouse(7)\n",
        see_also: &[b"pci", b"hwinfo", b"architecture", b"mouse"],
    },
    ManPage {
        section: 7, name: b"mouse", title: b"mouse - USB HID mouse cursor",
        content: b"SYNOPSIS\n  mouse\n\nDESCRIPTION\n  Displays current mouse cursor position and button state.\n  The mouse cursor is rendered as a 12x16 arrow bitmap on the\n  framebuffer. Background save/restore prevents flicker.\n\n  The mouse driver is part of the USB HID subsystem. It uses\n  EHCI periodic schedule interrupt transfers to poll the mouse\n  report at its configured interval (typically every 8-10 ms).\n\n  Boot protocol: 3-byte report.\n  Byte 0: buttons (bit0=left, bit1=right, bit2=middle)\n  Byte 1: X delta (signed, relative)\n  Byte 2: Y delta (signed, relative)\n\n  Cursor is automatically hidden when keyboard input is active\n  and redrawn on mouse movement.\n\nLIMITATIONS\n  Relative mode only (no absolute positioning).\n  No scroll wheel support.\n  No acceleration or smoothing.\n  Cursor clipped to framebuffer bounds.\n\nSEE ALSO\n  usb(7), pci(1), hwinfo(1)\n",
        see_also: &[b"usb", b"pci", b"hwinfo"],
    },
];

fn all_pages() -> &'static [&'static [ManPage]] {
    &[MAN1, MAN2, MAN3, MAN4, MAN5, MAN7]
}

unsafe fn show_index() {
    terminal::write(b"\n=== PureOS v0.4 Manual Pages ===\n");
    terminal::write(b"Usage: man [section] <topic>\n\n");

    let sections: [(&[u8], &[ManPage]); 6] = [
        (b"Section 1: User Commands", MAN1),
        (b"Section 2: System Calls", MAN2),
        (b"Section 3: Libraries", MAN3),
        (b"Section 4: System (Kernel)", MAN4),
        (b"Section 5: File Formats", MAN5),
        (b"Section 7: Miscellaneous", MAN7),
    ];

    for (header, pages) in &sections {
        terminal::write(b"\n");
        terminal::write(header);
        terminal::write(b"\n");
        for page in *pages {
            terminal::write(b"  ");
            terminal::write(page.name);
            terminal::write(b" - ");
            terminal::write(page.title);
            terminal::write(b"\n");
        }
    }
    terminal::write(b"\n");
}

unsafe fn find_page(name: &[u8], section: Option<u8>) -> Option<&'static ManPage> {
    let groups: &[&[ManPage]] = all_pages();
    for &group in groups {
        for page in group {
            let sec_match = section.map_or(true, |s| s == page.section);
            if sec_match && page.name.eq_ignore_ascii_case(name) {
                return Some(page);
            }
        }
    }
    None
}

/// List pages matching a section
unsafe fn list_section(section: u8) {
    let groups: &[&[ManPage]] = all_pages();
    let mut found = false;
    for &group in groups {
        for page in group {
            if page.section == section {
                if !found {
                    terminal::write(b"Section ");
                    terminal::write_num(section as u64);
                    terminal::write(b":\n\n");
                    found = true;
                }
                terminal::write(b"  ");
                terminal::write(page.name);
                terminal::write(b" - ");
                terminal::write(page.title);
                terminal::write(b"\n");
            }
        }
    }
    if !found {
        terminal::write(b"No pages in section ");
        terminal::write_num(section as u64);
        terminal::write(b"\n");
    }
}

unsafe fn show_page(page: &ManPage) {
    terminal::write(b"\n");
    terminal::write(page.title);
    terminal::write(b" (");
    terminal::write_num(page.section as u64);
    terminal::write(b")\n\n");
    terminal::write(page.content);
    if !page.see_also.is_empty() {
        terminal::write(b"\nSEE ALSO\n  ");
        for (i, refs) in page.see_also.iter().enumerate() {
            if i > 0 { terminal::write(b", "); }
            terminal::write(refs);
            terminal::write(b"(?)");
        }
        terminal::write(b"\n");
    }
    terminal::write(b"\n");
}

/// Main man entry point. Called from shell.
/// Supports: man, man <topic>, man <section> <topic>
pub unsafe fn man(args: &[u8]) {
    let trimmed = args.trim_ascii();
    if trimmed.is_empty() {
        return show_index();
    }

    // Check if first arg is a section number
    let mut space = None;
    for (i, &c) in trimmed.iter().enumerate() {
        if c == b' ' {
            space = Some(i);
            break;
        }
    }

    let (first, rest) = if let Some(pos) = space {
        (&trimmed[..pos], Some(&trimmed[pos+1..]))
    } else {
        (trimmed, None)
    };

    // If first arg is a digit, treat as section
    if first.len() == 1 && first[0] >= b'1' && first[0] <= b'9' {
        let sec = first[0] - b'0';
        if let Some(topic) = rest {
            // man <section> <topic>
            let topic = topic.trim_ascii();
            if let Some(page) = find_page(topic, Some(sec)) {
                show_page(page);
            } else {
                terminal::write(b"No entry for ");
                terminal::write(topic);
                terminal::write(b" in section ");
                terminal::write_num(sec as u64);
                terminal::write(b"\n");
            }
        } else {
            // man <section> - list section
            list_section(sec);
        }
        return;
    }

    // man <topic> - search all sections
    if let Some(page) = find_page(first, None) {
        show_page(page);
    } else {
        terminal::write(b"No manual entry for ");
        terminal::write(first);
        terminal::write(b"\n");
        terminal::write(b"Use 'man' to see available topics.\n");
    }
}

/// Deprecated: kept for backward compat. Redirects to man().
pub unsafe fn show_command_help(cmd: &[u8]) {
    man(cmd);
}

/// Show detailed info shell command. Redirects to man("info").
pub unsafe fn show_info() {
    if let Some(page) = find_page(b"info", None) {
        show_page(page);
    }
}
