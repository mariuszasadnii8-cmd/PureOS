//! Система локальной документации PureOS
//! Встроенная справочная система с иерархией и поиском

use crate::terminal;

/// Статья документации
pub struct DocArticle {
    pub title: &'static [u8],
    pub category: &'static [u8],
    pub content: &'static [u8],
    pub related: &'static [&'static [u8]],
}

/// Категории документации
const CAT_SYSTEM: &[u8] = b"System";
const CAT_COMMANDS: &[u8] = b"Commands";
const CAT_GRAPHICS: &[u8] = b"Graphics";
const CAT_PROGRAMMING: &[u8] = b"Programming";
const CAT_INSTALLATION: &[u8] = b"Installation";

/// База знаний PureOS
const DOCS: &[DocArticle] = &[
    DocArticle {
        title: b"Introduction",
        category: CAT_SYSTEM,
        content: b"PureOS is an immutable ephemeral kernel designed for RAM/ROM architectures with UEFI boot support for x86_64 AMD64.\n\nKey Features:\n- Zero-alloc architecture\n- Ephemeral memory layers\n- Built-in Barrel scripting language\n- Graphics support with automatic resolution detection\n- UEFI-only (no legacy BIOS support)\n",
        related: &[b"Architecture", b"Installation"],
    },
    DocArticle {
        title: b"Architecture",
        category: CAT_SYSTEM,
        content: b"PureOS Architecture Overview:\n\n1. Crystal Topology\n   - Static system topology frozen at boot\n   - CPU count, RAM/ROM configuration\n   - Immutable after initialization\n\n2. Ephemeral Layers\n   - Each process has isolated memory layer\n   - No shared mutable state\n   - Zero-copy IPC via rendezvous\n\n3. Process Model\n   - Round-robin scheduler\n   - Ring 3 user processes\n   - Ring 0 kernel process\n\n4. Memory Management\n   - No heap allocation in kernel\n   - Static frame allocator\n   - Identity-mapped kernel in all processes\n",
        related: &[b"Introduction", b"Memory", b"Processes"],
    },
    DocArticle {
        title: b"Memory",
        category: CAT_SYSTEM,
        content: b"Memory Management in PureOS:\n\nRAM/ROM Support:\n- Automatic detection at boot\n- Configurable via boot parameters\n- Separate base addresses for RAM and ROM\n\nEphemeral Layers:\n- 16MB per process\n- Identity-mapped in process space\n- Bump allocator (no free)\n- Isolated between processes\n\nFrame Allocator:\n- Static pool of 4KB frames\n- Allocated to ephemeral layers\n- No deallocation (ephemeral)\n",
        related: &[b"Architecture", b"Processes"],
    },
    DocArticle {
        title: b"Processes",
        category: CAT_SYSTEM,
        content: b"Process Management:\n\nProcess States:\n- Empty: unused slot\n- Runnable: ready to execute\n- BlockedOnSend: waiting for IPC send\n- BlockedOnReceive: waiting for IPC receive\n- BlockedOnReply: waiting for IPC reply\n- Exited: process terminated\n\nProcess Limits:\n- Maximum 64 processes\n- 16KB kernel stack per process\n- 64KB user stack per process\n- 16MB ephemeral layer per process\n\nSystem Calls:\n- SYSCALL/SYSRET for ring transitions\n- MSR-based entry points\n- Fast-path for common operations\n",
        related: &[b"Architecture", b"IPC", b"Syscalls"],
    },
    DocArticle {
        title: b"IPC",
        category: CAT_SYSTEM,
        content: b"Inter-Process Communication:\n\nRendezvous IPC:\n- No buffering in kernel\n- Direct copy between processes\n- Synchronous blocking semantics\n\nOperations:\n- send(target, message): send and block\n- receive(buffer): receive and block\n- reply(target, message): reply to sender\n\nMessage Size:\n- Fixed 64 bytes per message\n- Zero-copy transfer\n- Type-safe via process isolation\n",
        related: &[b"Processes", b"Syscalls"],
    },
    DocArticle {
        title: b"Syscalls",
        category: CAT_SYSTEM,
        content: b"System Call Reference:\n\nMemory (1-2):\n- memory_allocate(size, flags)\n- memory_free(addr)\n\nProcess (3-6):\n- create_process(entry)\n- create_thread(entry, stack)\n- yield_cpu()\n- exit_process(code)\n\nIPC (7-10):\n- send_ipc(target, message)\n- receive_ipc(buffer)\n- reply_ipc(target, message)\n- share_memory(target, addr, size)\n\nHardware (11-14):\n- pci_device_access(bus, offset)\n- map_physical_memory(addr, size)\n- create_shared_buffer(size, flags)\n- wait_for_vblank()\n\nGraphics (33-40):\n- get_screen_info(buf)\n- draw_pixel(x, y, color)\n- draw_line(x1, y1, x2, y2, color)\n- draw_rect(x, y, w, h, color, fill)\n- draw_circle(x, y, radius, color, fill)\n- draw_image(x, y, data, w, h)\n- clear_screen(color)\n- set_font_scale(scale)\n",
        related: &[b"Processes", b"Graphics"],
    },
    DocArticle {
        title: b"Graphics",
        category: CAT_GRAPHICS,
        content: b"Graphics Subsystem:\n\nAutomatic Resolution:\n- Detects screen resolution from UEFI GOP\n- Adapts font scale automatically\n- Supports any resolution\n\nGraphics Primitives:\n- draw_pixel: single pixel\n- draw_line: Bresenham algorithm\n- draw_rect: filled or outline\n- draw_circle: midpoint algorithm\n- draw_image: RGB24 buffer\n\nColor Format:\n- 24-bit RGB (0x00RRGGBB)\n- Named colors supported\n- Alpha channel not yet supported\n\nScreen Info:\n- Width, height, stride\n- Pixel format detection\n- Dynamic adaptation\n",
        related: &[b"Syscalls", b"Barrel Graphics"],
    },
    DocArticle {
        title: b"Barrel Graphics",
        category: CAT_PROGRAMMING,
        content: b"Graphics in Barrel:\n\nDrawing Functions:\n- draw_pixel(x, y, color)\n- draw_line(x1, y1, x2, y2, color)\n- draw_rect(x, y, w, h, color, fill)\n- draw_circle(x, y, r, color, fill)\n- clear_screen(color)\n\nExample:\n```\nlet red = 0xFF0000;\nlet blue = 0x0000FF;\nclear_screen blue;\ndraw_rect 100 100 200 100 red true;\n```\n\nColor Constants:\n- BLACK = 0x000000\n- WHITE = 0xFFFFFF\n- RED = 0xFF0000\n- GREEN = 0x00FF00\n- BLUE = 0x0000FF\n",
        related: &[b"Graphics", b"Barrel Language"],
    },
    DocArticle {
        title: b"Barrel Language",
        category: CAT_PROGRAMMING,
        content: b"Barrel Scripting Language:\n\nSyntax:\n- let name = value;\n- println expression;\n- if condition { ... } else { ... }\n- while condition { ... }\n\nData Types:\n- Integers (64-bit)\n- Strings (byte arrays)\n- Booleans\n\nFunctions:\n- fn name(args) { ... }\n- return value;\n\nExample:\n```\nlet x = 10;\nlet y = 20;\nif x < y {\n  println \"x is smaller\";\n} else {\n  println \"y is smaller\";\n}\n```\n\nCompilation:\n- cc \"source code\" - compile and run\n- Generates native ring3 code\n- No interpreter overhead\n",
        related: &[b"Barrel Graphics", b"Programming"],
    },
    DocArticle {
        title: b"Shell Commands",
        category: CAT_COMMANDS,
        content: b"Shell Command Reference:\n\nFile System:\n- pwd: show current directory\n- ls [-la]: list files\n- cd <path>: change directory\n- mkdir <name>: create directory\n- touch <file>: create file\n- rm [-rf] <file>: remove\n- cp <src> <dst>: copy\n- mv <src> <dst>: move/rename\n\nText:\n- cat <file>: display content\n- head <file>: first lines\n- tail <file>: last lines\n- grep <pattern> <file>: search\n\nSystem:\n- uname [-a]: system info\n- uptime: uptime\n- whoami: current user\n- ps: process list\n- kill <pid>: kill process\n- df [-h]: disk usage\n- free [-m]: memory usage\n\nUtilities:\n- history: command history\n- clear: clear screen\n- echo <text>: print text\n- man <cmd>: manual\n",
        related: &[b"System"],
    },
    DocArticle {
        title: b"Installation",
        category: CAT_INSTALLATION,
        content: b"PureOS Installation:\n\nRequirements:\n- UEFI 2.0+ firmware\n- x86_64 AMD64 processor\n- 512MB RAM minimum\n- 8GB disk space\n\nInstallation Steps:\n1. Boot PureOS installer\n2. Select target disk\n3. Choose partition scheme\n4. Create EFI System Partition (512MB)\n5. Install bootloader\n6. Copy system files\n7. Reboot\n\nEFI Partition:\n- Required for UEFI boot\n- FAT32 format\n- F12 boot menu support\n- Dual-boot compatible\n\nBootloader:\n- UEFI application\n- Loads kernel from ESP\n- Passes framebuffer info\n- Supports multiple kernels\n",
        related: &[b"Introduction", b"Architecture"],
    },
];

/// Показать оглавление документации
pub unsafe fn show_index() {
    terminal::write(b"\n=== PureOS Documentation Index ===\n\n");
    
    let mut current_cat: Option<&[u8]> = None;
    
    for doc in DOCS {
        if current_cat != Some(doc.category) {
            current_cat = Some(doc.category);
            terminal::write(b"\n[");
            terminal::write(doc.category);
            terminal::write(b"]\n");
        }
        
        terminal::write(b"  - ");
        terminal::write(doc.title);
        terminal::write(b"\n");
    }
    
    terminal::write(b"\nUse 'man <topic>' to view article\n");
}

/// Найти статью по названию
pub unsafe fn find_article(title: &[u8]) -> Option<&'static DocArticle> {
    for doc in DOCS {
        if doc.title.eq_ignore_ascii_case(title) {
            return Some(doc);
        }
    }
    None
}

/// Показать статью
pub unsafe fn show_article(title: &[u8]) {
    if let Some(doc) = find_article(title) {
        terminal::write(b"\n=== ");
        terminal::write(doc.title);
        terminal::write(b" ===\n\n");
        
        terminal::write(doc.content);
        
        if !doc.related.is_empty() {
            terminal::write(b"\nRelated: ");
            for (i, related) in doc.related.iter().enumerate() {
                if i > 0 {
                    terminal::write(b", ");
                }
                terminal::write(related);
            }
            terminal::write(b"\n");
        }
        
        terminal::write(b"\n");
    } else {
        terminal::write(b"Article not found: ");
        terminal::write(title);
        terminal::write(b"\n");
        terminal::write(b"Use 'man' to see available topics\n");
    }
}

/// Case-insensitive contains for byte slices
fn contains_ignore_case(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() { return true; }
    haystack.windows(needle.len()).any(|w| {
        w.iter().zip(needle.iter()).all(|(&a, &b)| a.eq_ignore_ascii_case(&b))
    })
}

/// Поиск по документации
pub unsafe fn search_docs(query: &[u8]) {
    terminal::write(b"\n=== Search Results for '");
    terminal::write(query);
    terminal::write(b"' ===\n\n");
    
    let mut found = false;
    
    for doc in DOCS {
        if contains_ignore_case(doc.title, query) || contains_ignore_case(doc.content, query) {
            if found {
                terminal::write(b"\n");
            }
            terminal::write(b"[");
            terminal::write(doc.category);
            terminal::write(b"] ");
            terminal::write(doc.title);
            terminal::write(b"\n");
            found = true;
        }
    }
    
    if !found {
        terminal::write(b"No results found.\n");
    }
    
    terminal::write(b"\n");
}

/// Показать справку по команде
pub unsafe fn show_command_help(cmd: &[u8]) {
    // Сначала ищем в документации
    if let Some(doc) = find_article(cmd) {
        show_article(cmd);
        return;
    }
    
    // Если не найдено, показываем базовую справку
    terminal::write(b"\nHelp for command: ");
    terminal::write(cmd);
    terminal::write(b"\n\n");
    
    match cmd {
        b"pwd" => terminal::write(b"pwd - Print working directory\nShows the current directory path.\n"),
        b"ls" => terminal::write(b"ls [-la] [path] - List directory contents\nOptions:\n  -l  long format\n  -a  include hidden files\n"),
        b"cd" => terminal::write(b"cd <path> - Change directory\nNavigate to the specified directory.\nUse 'cd ..' to go up one level.\n"),
        b"mkdir" => terminal::write(b"mkdir <name> - Create directory\nCreates a new directory with the specified name.\n"),
        b"touch" => terminal::write(b"touch <file> - Create empty file\nCreates a new empty file or updates timestamp.\n"),
        b"rm" => terminal::write(b"rm [-rf] <file> - Remove files or directories\nOptions:\n  -r  recursive (for directories)\n  -f  force (no confirmation)\n"),
        b"cp" => terminal::write(b"cp <src> <dst> - Copy files\nCopy source file to destination.\n"),
        b"mv" => terminal::write(b"mv <src> <dst> - Move/rename files\nMove or rename source to destination.\n"),
        b"cat" => terminal::write(b"cat <file> - Concatenate and display files\nDisplay the contents of a file.\n"),
        b"grep" => terminal::write(b"grep <pattern> <file> - Search in files\nSearch for a pattern in a file.\n"),
        b"install" => terminal::write(b"install - Start PureOS installer\nLaunch the installation wizard to install PureOS on disk.\n"),
        _ => {
            terminal::write(b"No specific help available.\n");
            terminal::write(b"Use 'man' to see all documentation topics.\n");
        }
    }
    
    terminal::write(b"\n");
}
