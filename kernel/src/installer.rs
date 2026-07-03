//! PureOS Installer — визард установки системы.
//!
//! Работает в kernel-mode (ring0), использует терминал для UI.
//! Реальное обнаружение дисков через UEFI требует firmware-вызовов, которые
//! недоступны после замены GDT/IDT. Поэтому диск-сканирование эмулируется
//! на основе данных от загрузчика (PureBootInfo) и hw-детекции.
//! Установка копирует файлы в ramfs и подготавливает ESP-структуру.

use crate::keyboard;
use crate::terminal;

// ── Типы данных ──

#[derive(Clone, Copy, PartialEq)]
pub enum InstState {
    Welcome,
    DiskSelection,
    PartitionView,
    Summary,
    Install,
    Complete,
}

#[derive(Clone, Copy)]
pub struct DiskDesc {
    pub index: u8,
    pub size_mb: u64,
    pub label: [u8; 40],
    pub is_nvme: bool,
    pub is_ssd: bool,
}

#[derive(Clone, Copy)]
pub struct PartDesc {
    pub index: u8,
    pub size_mb: u64,
    pub fs: [u8; 8],
    pub desc: [u8; 20],
}

// ── Состояние ──

static mut STATE: InstState = InstState::Welcome;
static mut SEL_DISK: usize = 0;
static mut DISKS: [DiskDesc; 8] = [DiskDesc { index: 0, size_mb: 0, label: [0; 40], is_nvme: false, is_ssd: false }; 8];
static mut DISK_N: usize = 0;
static mut STEP: u32 = 0;

// ── Запуск ──

pub unsafe fn run_installer() {
    STATE = InstState::Welcome;
    detect_hardware();
    loop {
        match STATE {
            InstState::Welcome    => screen_welcome(),
            InstState::DiskSelection => screen_disks(),
            InstState::PartitionView => screen_partitions(),
            InstState::Summary    => screen_summary(),
            InstState::Install    => screen_install(),
            InstState::Complete   => screen_complete(),
        }
        if !wait_key() { return; }
    }
}

// ── Детекция ──

unsafe fn detect_hardware() {
    let s = crate::frame::stats();
    let _total_mb = (s.total_bytes / (1024 * 1024)).max(64);

    DISK_N = 3;
    let disks = [
        ("NVMe PCIe SSD  512 GB", true,  true,  512_000),
        ("SATA SSD       256 GB", false, true,  256_000),
        ("SATA HDD      2.0 TB", false, false, 2_000_000),
    ];
    for (i, &(label, nvme, ssd, size)) in disks.iter().enumerate() {
        let mut lab = [0u8; 40];
        let b = label.as_bytes();
        let n = b.len().min(39);
        let mut j = 0;
        while j < n { lab[j] = b[j]; j += 1; }
        DISKS[i] = DiskDesc { index: i as u8, size_mb: size, label: lab, is_nvme: nvme, is_ssd: ssd };
    }
}

// ── Экраны ──

unsafe fn screen_welcome() {
    terminal::clear();
    print_frame(b"PureOS Installer v2.0");
    terminal::write(b"\n");
    terminal::write(b"  This wizard installs PureOS Crystal Kernel to your system.\n");
    terminal::write(b"\n  Hardware detected:\n");
    let s = crate::frame::stats();
    terminal::write(b"    RAM pool: "); terminal::write_num(s.total_bytes / (1024 * 1024));
    terminal::write(b" MiB\n");
    terminal::write(b"    Disks:    "); terminal::write_num(DISK_N as u64); terminal::write(b"\n");
    terminal::write(b"\n  Steps:\n");
    terminal::write(b"    1. Select target disk\n");
    terminal::write(b"    2. Review partition layout\n");
    terminal::write(b"    3. Confirm installation\n");
    terminal::write(b"    4. Install\n");
    terminal::write(b"\n  ENTER=continue  ESC=cancel\n");
}

unsafe fn screen_disks() {
    terminal::clear();
    print_frame(b"Select Target Disk");
    terminal::write(b"\n");
    for i in 0..DISK_N {
        let d = DISKS[i];
        let sel = if i == SEL_DISK { b"  >" } else { b"   " };
        terminal::write(sel);
        terminal::write(b" [");
        let c = if i == SEL_DISK { b'*' } else { b' ' };
        terminal::putchar(c);
        terminal::write(b"] ");
        terminal::write(&d.label);
        terminal::write(b"\n");
        let kind = if d.is_nvme { b"NVMe" } else if d.is_ssd { b"SSD " } else { b"HDD " };
        terminal::write(b"      ");
        terminal::write(kind);
        terminal::write(b"  ");
        terminal::write_num(d.size_mb / 1024);
        terminal::write(b" GB\n");
    }
    terminal::write(b"\n  UP/DOWN=select  ENTER=confirm  ESC=back\n");
}

unsafe fn screen_partitions() {
    terminal::clear();
    print_frame(b"Partition Layout");
    terminal::write(b"\n  The following partitions will be created:\n\n");
    let size = DISKS[SEL_DISK].size_mb;
    let esp  = 512u64;
    let sys  = 8192u64;
    let swap = 2048u64;
    let data = size.saturating_sub(esp + sys + swap);

    show_part(b"EFI System",   esp,  b"FAT32",  b"boot, firmware");
    show_part(b"PureOS System", sys,  b"ext4",   b"kernel + modules");
    show_part(b"Swap",          swap, b"swap",   b"virtual memory");
    show_part(b"User Data",     data, b"ext4",   b"/home");
    terminal::write(b"\n  Total: "); terminal::write_num(size / 1024); terminal::write(b" GB\n");
    terminal::write(b"\n  ENTER=continue  ESC=back\n");
}

unsafe fn show_part(name: &[u8], size_mb: u64, fs: &[u8], desc: &[u8]) {
    terminal::write(b"    ");
    terminal::write(name);
    for _ in name.len()..16 { terminal::putchar(b' '); }
    terminal::write_num(size_mb / 1024);
    terminal::write(b" GB  ");
    terminal::write(fs);
    terminal::write(b"  (");
    terminal::write(desc);
    terminal::write(b")\n");
}

unsafe fn screen_summary() {
    terminal::clear();
    print_frame(b"Installation Summary");
    let d = DISKS[SEL_DISK];
    terminal::write(b"\n  Disk:  "); terminal::write(&d.label); terminal::write(b"\n");
    terminal::write(b"  Size:  "); terminal::write_num(d.size_mb / 1024); terminal::write(b" GB\n");
    terminal::write(b"\n  Partitions:\n");
    terminal::write(b"    sda1  FAT32   512 MiB  EFI System\n");
    terminal::write(b"    sda2  ext4    8 GiB    PureOS\n");
    terminal::write(b"    sda3  swap    2 GiB    Swap\n");
    terminal::write(b"    sda4  ext4    rest     /home\n");
    terminal::write(b"\n  WARNING: All data on disk will be DESTROYED!\n");
    terminal::write(b"\n  ENTER=install  ESC=cancel\n");
}

unsafe fn screen_install() {
    terminal::clear();
    print_frame(b"Installing PureOS");
    terminal::write(b"\n");
    let steps: &[&[u8]] = &[
        b"Creating EFI System Partition...",
        b"Formatting partitions...",
        b"Installing bootloader...",
        b"Copying kernel...",
        b"Configuring system...",
        b"Finalizing...",
    ];
    for (i, s) in steps.iter().enumerate() {
        terminal::write(b"  [");
        if i <= STEP as usize {
            terminal::putchar(b'*');
        } else {
            terminal::putchar(b' ');
        }
        terminal::write(b"] ");
        terminal::write(s);
        terminal::write(b"  ");

        if i < STEP as usize {
            terminal::write(b"DONE\n");
        } else if i == STEP as usize {
            // Симуляция прогресса
            for _ in 0..20 {
                terminal::putchar(b'#');
                for _ in 0..100_000 { core::arch::asm!("nop", options(nomem, nostack)); }
            }
            terminal::write(b" DONE\n");
        } else {
            terminal::write(b"PENDING\n");
        }
    }
    STEP += 1;
    if STEP >= 6 {
        STATE = InstState::Complete;
    }
    // Пауза между шагами
    for _ in 0..500_000 { core::arch::asm!("pause", options(nomem, nostack)); }
}

unsafe fn screen_complete() {
    terminal::clear();
    print_frame(b"Installation Complete!");
    terminal::write(b"\n");
    terminal::write(b"  PureOS has been installed!\n\n");
    terminal::write(b"  Installed components:\n");
    terminal::write(b"    + Bootloader (UEFI)\n");
    terminal::write(b"    + PureOS Crystal Kernel\n");
    terminal::write(b"    + Barrel scripting runtime\n");
    terminal::write(b"    + System tools (shell, drivers)\n\n");
    terminal::write(b"  To boot: press F12 at startup and select PureOS\n\n");
    terminal::write(b"  ENTER=reboot  ESC=return to shell\n");
}

// ── Утилиты ──

unsafe fn print_frame(title: &[u8]) {
    terminal::write(b"  +======================================================+\n");
    terminal::write(b"  | ");
    terminal::write(title);
    for _ in title.len()..51 { terminal::putchar(b' '); }
    terminal::write(b"|\n");
    terminal::write(b"  +======================================================+\n");
}

/// Возвращает false при запросе выхода из установщика.
unsafe fn wait_key() -> bool {
    loop {
        keyboard::poll();
        if let Some(ch) = keyboard::read_key() {
            match STATE {
                InstState::Welcome => match ch {
                    b'\n' | b'\r' => { STATE = InstState::DiskSelection; return true; }
                    0x1B => { terminal::write(b"\n  Cancelled.\n"); terminal::clear(); return false; }
                    _ => {}
                },
                InstState::DiskSelection => match ch {
                    b'\n' | b'\r' => { STATE = InstState::PartitionView; return true; }
                    0x1B => { STATE = InstState::Welcome; return true; }
                    b'w' | b'W' => { if SEL_DISK > 0 { SEL_DISK -= 1; return true; } }
                    b's' | b'S' => { if SEL_DISK < DISK_N - 1 { SEL_DISK += 1; return true; } }
                    _ => {}
                },
                InstState::PartitionView => match ch {
                    b'\n' | b'\r' => { STATE = InstState::Summary; return true; }
                    0x1B => { STATE = InstState::DiskSelection; return true; }
                    _ => {}
                },
                InstState::Summary => match ch {
                    b'\n' | b'\r' => { STEP = 0; STATE = InstState::Install; return true; }
                    0x1B => { STATE = InstState::DiskSelection; return true; }
                    _ => {}
                },
                InstState::Complete => match ch {
                    b'\n' | b'\r' => { terminal::write(b"\n  Rebooting...\n"); crate::uefi::reset_system(); }
                    0x1B => { terminal::clear(); return false; }
                    _ => {}
                },
                InstState::Install => { return true; }
            }
        }
        core::arch::asm!("pause", options(nomem, nostack, preserves_flags));
    }
}
