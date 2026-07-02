# PureOS Architecture

## System Overview

PureOS is an immutable ephemeral kernel designed for RAM/ROM architectures with UEFI boot support for x86_64 AMD64.

## Core Principles

1. **Zero-Alloc Architecture** - No heap allocation in kernel
2. **Immutable Topology** - System configuration frozen at boot
3. **Ephemeral Layers** - Isolated memory per process
4. **UEFI-Only** - No legacy BIOS support
5. **Static Memory** - All kernel state in static memory

## System Hierarchy

```
PureOS/
├── Hardware Layer
│   ├── UEFI Firmware
│   ├── x86_64 AMD64 CPU
│   └── RAM/ROM Memory
├── Boot Layer
│   ├── UEFI Bootloader (uefi_boot/)
│   └── PureOS Kernel (kernel/)
├── Kernel Layer
│   ├── Core Subsystems
│   │   ├── Process Manager (syscall.rs)
│   │   ├── Memory Manager (frame.rs, ephemeral.rs)
│   │   ├── Scheduler (round-robin)
│   │   └── IPC System (rendezvous)
│   ├── Graphics Subsystem
│   │   ├── Framebuffer (framebuffer.rs)
│   │   ├── Graphics Primitives (graphics.rs)
│   │   └── Font Rendering (font.rs)
│   ├── I/O Subsystems
│   │   ├── Terminal (terminal.rs)
│   │   ├── Keyboard (keyboard.rs)
│   │   └── Console (console.rs)
│   ├── System Services
│   │   ├── Installer (installer.rs)
│   │   ├── Documentation (documentation.rs)
│   │   └── Shell (shell.rs, commands.rs)
│   └── Programming
│       ├── Barrel REPL (barrel.rs)
│       └── Barrel Compiler (barrelc.rs)
└── Userspace Layer
    ├── C Programs (userspace/sys_c/)
    ├── ASM Programs (userspace/sys_asm/)
    └── Barrel Scripts
```

## Memory Architecture

### Address Space Layout

```
0x0000_0000_0000_0000 - 0x0000_0000_1000_0000  Physical Memory (Identity-mapped)
0x0000_1000_0000_0000 - 0x0000_1001_0000_0000  Ephemeral Layers (16MB × 64 processes)
0xFFFF_8000_0000_0000 - 0xFFFF_FFFF_FFFF_FFFF  Kernel Code/Data
```

### Memory Types

1. **RAM** - Main system memory detected at boot
2. **ROM** - Read-only memory for firmware/code storage
3. **Ephemeral Layers** - Per-process isolated memory (16MB each)
4. **Frame Pool** - Static 4KB frame allocator

### Process Memory Layout

```
┌─────────────────────────────────┐
│   Kernel Stack (16KB)           │ Ring 0
├─────────────────────────────────┤
│   User Stack (64KB)             │ Ring 3
├─────────────────────────────────┤
│   Code/Data                    │ Ring 3
├─────────────────────────────────┤
│   Ephemeral Layer (16MB)        │ Ring 3
│   - Bump allocator             │
│   - No free()                   │
│   - Process isolation          │
└─────────────────────────────────┘
```

## Process Architecture

### Process States

```
Empty → Runnable → [BlockedOnSend/Receive/Reply] → Exited
         ↑              ↓
         └──────────────┘ (yield/schedule)
```

### Process Control Block

```rust
struct ProcessControlBlock {
    id: u64,
    state: ProcessState,
    page_table_base: u64,
    entry: u64,
    user_stack: u64,
    layer_base: u64,
    layer_size: u64,
    ipc_buffer: u64,
    ipc_peer: u64,
    saved_rsp: u64,
    kernel_stack_top: u64,
    exit_code: u64,
}
```

### Process Limits

- Maximum processes: 64
- Kernel stack per process: 16KB
- User stack per process: 64KB
- Ephemeral layer per process: 16MB
- IPC message size: 64 bytes

## System Call Architecture

### System Call Entry

1. User executes `syscall` instruction
2. Hardware switches to kernel mode via MSR
3. `syscall_entry` trampoline saves context
4. `sys_call_handler` dispatches to handler
5. Handler executes and returns result
6. `sysretq` returns to user mode

### System Call Categories

1. **Memory (1-2)**: allocate, free
2. **Process (3-6)**: create, thread, yield, exit
3. **IPC (7-10)**: send, receive, reply, share
4. **Hardware (11-14)**: PCI, physical memory, shared buffer, vblank
5. **File I/O (16-23)**: write, read, open, close, lseek, stat, dup, fcntl
6. **Magic (24-32)**: print, println, input, ticks, cls, cursor, color, reboot, print_num
7. **Graphics (33-40)**: screen info, pixel, line, rect, circle, image, clear, font scale

## Graphics Architecture

### Graphics Pipeline

```
User Application
    ↓ (Graphics Syscalls)
Graphics Subsystem (graphics.rs)
    ↓ (Primitives)
Framebuffer (framebuffer.rs)
    ↓ (Pixel Data)
UEFI GOP
    ↓
Physical Display
```

### Graphics Features

1. **Automatic Resolution Detection** - Adapts to any screen resolution
2. **Adaptive Font Scaling** - Adjusts font size based on resolution
3. **Graphics Primitives** - Pixel, line, rectangle, circle, image
4. **Color Support** - 24-bit RGB with named colors
5. **Zero-Alloc Rendering** - All rendering in static memory

### Resolution Adaptation

- 1920×1080+: Font scale 2
- 1366×768: Font scale 1
- 1024×768: Font scale 1
- Lower: Font scale 1 (minimum)

## IPC Architecture

### Rendezvous IPC Model

```
Process A                    Process B
    |                            |
    | send_ipc(target, msg)      |
    |──────────────────────────> |
    | [blocked]                  |
    |                            | receive_ipc(buf)
    |                            | [blocked]
    |──────────────────────────> |
    | [direct copy]              |
    |                            | [unblocked, returns A]
    | [unblocked]                |
    | reply_ipc(target, msg)     |
    |──────────────────────────> |
    | [direct copy]              |
    |                            | [unblocked]
    | [unblocked]                |
```

### IPC Properties

- No buffering in kernel
- Direct copy between processes
- Synchronous blocking semantics
- Fixed 64-byte message size
- Type-safe via process isolation

## Boot Architecture

### Boot Sequence

```
UEFI Firmware
    ↓
UEFI Bootloader (uefi_boot/)
    ↓ Load kernel image
PureOS Kernel (kernel/)
    ↓ Initialize subsystems
├── CPU (GDT/TSS/IDT)
├── Memory (Frame allocator)
├── Process Manager
├── Syscall MSRs
├── Keyboard
└── Graphics
    ↓
Shell (shell.rs)
    ↓
Userspace Processes
```

### Boot Info Structure

```rust
struct PureBootInfo {
    magic: u64,
    kernel_base: u64,
    kernel_size: u64,
    framebuffer_base: u64,
    framebuffer_size: u64,
    framebuffer_width: u32,
    framebuffer_height: u32,
    framebuffer_stride: u32,
    framebuffer_format: u32,
    heap_base: u64,
    heap_size: u64,
    system_table: u64,
    con_in: u64,
}
```

## Installation Architecture

### Installer Wizard

```
Welcome Screen
    ↓
Disk Selection
    ↓
Partition Selection
    ↓
Partition Creation
    ├─ EFI System Partition (512MB, FAT32)
    ├─ PureOS System (8GB)
    ├─ Swap (2GB)
    └─ User Data (Remaining)
    ↓
Installation
    ├─ Create partitions
    ├─ Format filesystems
    ├─ Install bootloader
    ├─ Copy kernel files
    └─ Configure system
    ↓
Complete
```

### EFI Partition

- Required for UEFI boot
- FAT32 format
- 512MB minimum
- F12 boot menu support
- Dual-boot compatible

## Programming Architecture

### Barrel Language

```
Syntax:
  let name = value;
  println expression;
  if condition { ... } else { ... }
  while condition { ... }
  fn name(args) { ... }

Data Types:
  Integers (64-bit)
  Strings (byte arrays)
  Booleans

Compilation:
  cc "source code" - compile and run
  Generates native ring3 code
  No interpreter overhead
```

### Barrel Graphics

```
Graphics Functions:
  draw_pixel(x, y, color)
  draw_line(x1, y1, x2, y2, color)
  draw_rect(x, y, w, h, color, fill)
  draw_circle(x, y, r, color, fill)
  clear_screen(color)

Color Constants:
  BLACK = 0x000000
  WHITE = 0xFFFFFF
  RED = 0xFF0000
  GREEN = 0x00FF00
  BLUE = 0x0000FF
```

## Customization Architecture

### Configuration Points

1. **Boot Parameters** - Via boot_info structure
2. **Runtime Settings** - Via syscalls
3. **Graphics Settings** - Font scale, colors
4. **System Limits** - Process count, memory sizes
5. **Shell Configuration** - Commands, aliases

### Extension Points

1. **System Calls** - Add new syscall numbers
2. **Graphics Primitives** - Add new drawing functions
3. **Shell Commands** - Add new commands in commands.rs
4. **Barrel Functions** - Add new Barrel built-ins
5. **Documentation** - Add new articles in documentation.rs

## Security Architecture

### Memory Isolation

- Kernel mapped identically in all processes
- User processes have isolated address spaces
- Ephemeral layers prevent shared mutable state
- Ring 0/Ring 3 separation via SYSCALL/SYSRET

### Process Isolation

- IPC requires explicit send/receive
- No shared memory by default
- Type-safe message passing
- Process table isolation

### Boot Security

- UEFI Secure Boot compatible
- No legacy BIOS code
- Signed bootloader support (future)
- Measured boot support (future)

## Performance Characteristics

### Memory Performance

- Zero-alloc kernel (no heap overhead)
- Static memory allocation (no fragmentation)
- Identity-mapped kernel (no TLB misses)
- Direct IPC copy (no buffering)

### Scheduling Performance

- Round-robin O(1) per switch
- Coopertive (no timer overhead)
- Context switch ~100 cycles
- No preemption overhead

### Graphics Performance

- Direct framebuffer access
- No compositor overhead
- Adaptive rendering
- Zero-copy operations

## Future Extensions

### Planned Features

1. **True File System** - ext4/FAT32 support
2. **Network Stack** - TCP/IP networking
3. **Sound System** - Audio output
4. **USB Support** - USB device drivers
5. **ACPI Support** - Power management
6. **SMP Support** - Multi-core scheduling
7. **Virtualization** - Hypervisor support
8. **Security** - SELinux-like policies

### Research Areas

1. **Formal Verification** - Prove kernel correctness
2. **Capability Systems** - Fine-grained permissions
3. **Persistent Memory** - NVM/CXL optimization
4. **Real-time** - Deterministic scheduling
5. **Microkernel** - Further component isolation
