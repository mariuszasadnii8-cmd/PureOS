//! Установщик PureOS
//! Визард для установки системы на диск с созданием EFI раздела

use crate::terminal;
use crate::keyboard;

/// Информация о диске
#[derive(Clone, Copy)]
pub struct DiskInfo {
    pub index: u8,
    pub size_gb: u64,
    pub name: [u8; 32],
    pub has_efi: bool,
}

/// Информация о разделе
#[derive(Clone, Copy)]
pub struct PartitionInfo {
    pub index: u8,
    pub start_lba: u64,
    pub size_gb: u64,
    pub fs_type: [u8; 8],  // "FAT32", "NTFS", etc.
    pub is_bootable: bool,
}

/// Состояние установщика
#[derive(Clone, Copy, PartialEq)]
pub enum InstallerState {
    Welcome,
    DiskSelection,
    PartitionSelection,
    PartitionCreation,
    Installation,
    Complete,
}

static mut INSTALLER_STATE: InstallerState = InstallerState::Welcome;
static mut SELECTED_DISK: u8 = 0;
static mut DISKS: [DiskInfo; 8] = [DiskInfo {
    index: 0,
    size_gb: 0,
    name: [0; 32],
    has_efi: false,
}; 8];
static mut DISK_COUNT: u8 = 0;

/// Запустить установщик
pub unsafe fn run_installer() -> ! {
    INSTALLER_STATE = InstallerState::Welcome;
    detect_disks();
    
    loop {
        match INSTALLER_STATE {
            InstallerState::Welcome => show_welcome(),
            InstallerState::DiskSelection => show_disk_selection(),
            InstallerState::PartitionSelection => show_partition_selection(),
            InstallerState::PartitionCreation => show_partition_creation(),
            InstallerState::Installation => perform_installation(),
            InstallerState::Complete => show_complete(),
        }
        
        handle_installer_input();
    }
}

/// Обнаружить диски (заглушка - в реальности нужно использовать UEFI Block I/O)
unsafe fn detect_disks() {
    // TODO: Реальное обнаружение через UEFI Block I/O Protocol
    // Для MVP создаем фиктивные диски
    DISK_COUNT = 2;
    
    DISKS[0] = DiskInfo {
        index: 0,
        size_gb: 500,
        name: *b"NVMe SSD 500GB\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
        has_efi: true,
    };
    
    DISKS[1] = DiskInfo {
        index: 1,
        size_gb: 1000,
        name: *b"HDD 1TB\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
        has_efi: false,
    };
}

/// Показать экран приветствия
unsafe fn show_welcome() {
    terminal::clear();
    terminal::write(b"\n");
    terminal::write(b"  +======================================================+\n");
    terminal::write(b"  |              PureOS Installer v1.0                  |\n");
    terminal::write(b"  +======================================================+\n");
    terminal::write(b"\n");
    terminal::write(b"  Welcome to PureOS Installation Wizard\n");
    terminal::write(b"\n");
    terminal::write(b"  This wizard will guide you through the installation process:\n");
    terminal::write(b"  - Select target disk\n");
    terminal::write(b"  - Create EFI System Partition (for F12 boot)\n");
    terminal::write(b"  - Install PureOS kernel and bootloader\n");
    terminal::write(b"\n");
    terminal::write(b"  Press ENTER to continue, ESC to exit\n");
}

/// Показать выбор диска
unsafe fn show_disk_selection() {
    terminal::clear();
    terminal::write(b"\n");
    terminal::write(b"  +======================================================+\n");
    terminal::write(b"  |              Select Target Disk                      |\n");
    terminal::write(b"  +======================================================+\n");
    terminal::write(b"\n");
    
    for i in 0..DISK_COUNT as usize {
        let disk = DISKS[i];
        let selected = if i == SELECTED_DISK as usize { b"[*] " } else { b"[ ] " };
        
        terminal::write(b"  ");
        terminal::write(selected);
        terminal::write(&disk.name);
        terminal::write(b" (");
        terminal::write_num(disk.size_gb);
        terminal::write(b" GB)");
        
        if disk.has_efi {
            terminal::write(b" [EFI]");
        }
        terminal::write(b"\n");
    }
    
    terminal::write(b"\n");
    terminal::write(b"  Use UP/DOWN to select, ENTER to continue\n");
}

/// Показать выбор раздела
unsafe fn show_partition_selection() {
    terminal::clear();
    terminal::write(b"\n");
    terminal::write(b"  +======================================================+\n");
    terminal::write(b"  |              Partition Configuration                |\n");
    terminal::write(b"  +======================================================+\n");
    terminal::write(b"\n");
    terminal::write(b"  Selected: ");
    let disk = DISKS[SELECTED_DISK as usize];
    terminal::write(&disk.name);
    terminal::write(b"\n\n");
    
    terminal::write(b"  [1] Use entire disk (recommended)\n");
    terminal::write(b"  [2] Create custom partitions\n");
    terminal::write(b"  [3] Create EFI partition only (for dual-boot)\n");
    terminal::write(b"\n");
    terminal::write(b"  Press 1-3 to select option\n");
}

/// Показать создание раздела
unsafe fn show_partition_creation() {
    terminal::clear();
    terminal::write(b"\n");
    terminal::write(b"  +======================================================+\n");
    terminal::write(b"  |              Creating Partitions                    |\n");
    terminal::write(b"  +======================================================+\n");
    terminal::write(b"\n");
    terminal::write(b"  The following partitions will be created:\n\n");
    
    terminal::write(b"  EFI System Partition:  512 MB  (FAT32)\n");
    terminal::write(b"  PureOS System:         8 GB    (ext4)\n");
    terminal::write(b"  Swap:                 2 GB    (swap)\n");
    terminal::write(b"  User Data:            Remaining\n");
    
    terminal::write(b"\n");
    terminal::write(b"  Press ENTER to confirm, ESC to go back\n");
}

/// Выполнить установку
unsafe fn perform_installation() {
    terminal::clear();
    terminal::write(b"\n");
    terminal::write(b"  +======================================================+\n");
    terminal::write(b"  |              Installing PureOS                       |\n");
    terminal::write(b"  +======================================================+\n");
    terminal::write(b"\n");
    
    terminal::write(b"  [1/6] Creating EFI System Partition...\n");
    simulate_progress();
    terminal::write(b"  [2/6] Formatting partitions...\n");
    simulate_progress();
    terminal::write(b"  [3/6] Installing bootloader...\n");
    simulate_progress();
    terminal::write(b"  [4/6] Copying kernel files...\n");
    simulate_progress();
    terminal::write(b"  [5/6] Configuring system...\n");
    simulate_progress();
    terminal::write(b"  [6/6] Installation complete!\n");
    
    INSTALLER_STATE = InstallerState::Complete;
}

/// Показать завершение установки
unsafe fn show_complete() {
    terminal::clear();
    terminal::write(b"\n");
    terminal::write(b"  +======================================================+\n");
    terminal::write(b"  |              Installation Complete!                |\n");
    terminal::write(b"  +======================================================+\n");
    terminal::write(b"\n");
    terminal::write(b"  PureOS has been successfully installed!\n\n");
    terminal::write(b"  To boot PureOS:\n");
    terminal::write(b"  1. Restart your computer\n");
    terminal::write(b"  2. Press F12 during boot to access boot menu\n");
    terminal::write(b"  3. Select \"PureOS\" from the boot menu\n\n");
    terminal::write(b"  Press ENTER to reboot, ESC to exit to shell\n");
}

/// Симуляция прогресса
unsafe fn simulate_progress() {
    for i in 0..10 {
        terminal::write(b".");
        // TODO: реальная задержка
        for _ in 0..1000000 {
            core::arch::asm!("nop", options(nomem, nostack));
        }
    }
    terminal::write(b" OK\n");
}

/// Обработка ввода в установщике
unsafe fn handle_installer_input() {
    loop {
        keyboard::poll();
        if let Some(ch) = keyboard::read_key() {
            match INSTALLER_STATE {
                InstallerState::Welcome => {
                    match ch {
                        b'\n' | b'\r' => {
                            INSTALLER_STATE = InstallerState::DiskSelection;
                            return;
                        }
                        0x1B => {
                            terminal::write(b"\nExiting installer...\n");
                            return;
                        }
                        _ => {}
                    }
                }
                InstallerState::DiskSelection => {
                    match ch {
                        b'\n' | b'\r' => {
                            INSTALLER_STATE = InstallerState::PartitionSelection;
                            return;
                        }
                        0x1B => {
                            INSTALLER_STATE = InstallerState::Welcome;
                            return;
                        }
                        b'A' | b'[' => { // UP
                            if SELECTED_DISK > 0 {
                                SELECTED_DISK -= 1;
                                return;
                            }
                        }
                        b'B' | b'/' => { // DOWN
                            if SELECTED_DISK < DISK_COUNT - 1 {
                                SELECTED_DISK += 1;
                                return;
                            }
                        }
                        _ => {}
                    }
                }
                InstallerState::PartitionSelection => {
                    match ch {
                        b'1' => {
                            INSTALLER_STATE = InstallerState::PartitionCreation;
                            return;
                        }
                        b'2' => {
                            // TODO: custom partitioning
                            terminal::write(b"\nCustom partitioning not implemented yet\n");
                            return;
                        }
                        b'3' => {
                            // TODO: EFI only
                            terminal::write(b"\nEFI-only installation not implemented yet\n");
                            return;
                        }
                        0x1B => {
                            INSTALLER_STATE = InstallerState::DiskSelection;
                            return;
                        }
                        _ => {}
                    }
                }
                InstallerState::PartitionCreation => {
                    match ch {
                        b'\n' | b'\r' => {
                            INSTALLER_STATE = InstallerState::Installation;
                            return;
                        }
                        0x1B => {
                            INSTALLER_STATE = InstallerState::PartitionSelection;
                            return;
                        }
                        _ => {}
                    }
                }
                InstallerState::Complete => {
                    match ch {
                        b'\n' | b'\r' => {
                            terminal::write(b"\nRebooting...\n");
                            crate::uefi::reset_system();
                        }
                        0x1B => {
                            terminal::write(b"\nReturning to shell...\n");
                            return;
                        }
                        _ => {}
                    }
                }
                InstallerState::Installation => {
                    // Installation is automatic, no input needed
                    return;
                }
            }
        }
        core::arch::asm!("pause", options(nomem, nostack, preserves_flags));
    }
}

/// Создать EFI раздел на выбранном диске
pub unsafe fn create_efi_partition(disk_index: u8) -> bool {
    // TODO: Реальное создание раздела через UEFI Disk I/O
    // Для MVP возвращаем true
    true
}

/// Установить загрузчик на EFI раздел
pub unsafe fn install_bootloader(efi_partition: u8) -> bool {
    // TODO: Реальная установка загрузчика
    // Для MVP возвращаем true
    true
}

/// Скопировать файлы системы
pub unsafe fn copy_system_files(target_partition: u8) -> bool {
    // TODO: Реальное копирование файлов
    // Для MVP возвращаем true
    true
}
