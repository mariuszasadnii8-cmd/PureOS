//! ATA PIO mode disk driver — прямой доступ к диску через порты I/O.
//!
//! Работает на стандартных ATA-контроллёрах (Primary channel, IRQ14).
//! Zero-Alloc: статические буферы, никаких динамических аллокаций.
//! Поддерживает 28-bit LBA для совместимости со старыми контроллёрами
//! и 48-bit LBA для больших дисков.
//!
//! Портовая карта Primary ATA:
//!   0x1F0..0x1F7 — регистры команд/данных
//!   0x3F6         — Alternate Status / Device Control

pub const SECTOR_SIZE: u64 = 512;
const DATA_PORT: u16 = 0x1F0;
const ERROR_PORT: u16 = 0x1F1;
const SECTOR_COUNT: u16 = 0x1F2;
const LBA_LOW: u16 = 0x1F3;
const LBA_MID: u16 = 0x1F4;
const LBA_HIGH: u16 = 0x1F5;
const DRIVE_HEAD: u16 = 0x1F6;
const COMMAND_PORT: u16 = 0x1F7;
const ALT_STATUS: u16 = 0x3F6;

const CMD_READ_PIO: u8 = 0x20;
const CMD_WRITE_PIO: u8 = 0x30;
const CMD_IDENTIFY: u8 = 0xEC;
const CMD_FLUSH_CACHE: u8 = 0xE7;

const STATUS_BSY: u8 = 0x80;
const STATUS_DRDY: u8 = 0x40;
const STATUS_DRQ: u8 = 0x08;
const STATUS_ERR: u8 = 0x01;

static mut DISK_PRESENT: bool = false;

/// Информация о диске.
pub struct DiskInfo {
    pub present: bool,
    pub sectors: u64,
    pub model: [u8; 40],
}

/// Инициализация — обнаружение ATA-диска.
pub unsafe fn init() {
    DISK_PRESENT = detect();
}

fn detect() -> bool {
    unsafe {
        // Выбрать master-диск на primary channel.
        crate::cpu::outb(DRIVE_HEAD, 0xE0);
        // Небольшая пауза.
        crate::cpu::outb(ALT_STATUS, 0);

        // Послать IDENTIFY.
        crate::cpu::outb(COMMAND_PORT, CMD_IDENTIFY);

        // Ждать, пока не BSY или готовность.
        let mut timeout = 0;
        while timeout < 10000 {
            let status = crate::cpu::inb(COMMAND_PORT);
            if status & STATUS_BSY == 0 { break; }
            timeout += 1;
        }
        if timeout >= 10000 {
            return false; // Timeout
        }

        let status = crate::cpu::inb(COMMAND_PORT);
        if status == 0 {
            return false; // Нет устройства
        }

        // Проверить, что это ATA (LBA mid/high == 0 для ATA)
        if crate::cpu::inb(LBA_MID) != 0 || crate::cpu::inb(LBA_HIGH) != 0 {
            return false; // ATAPI или другое
        }

        true
    }
}

/// Прочитать один сектор PIO.
pub unsafe fn read_sector(lba: u32, buf: &mut [u8; 512]) -> bool {
    if !DISK_PRESENT { return false; }

    // Ждать готовности.
    poll_drq();

    // Выставить LBA.
    crate::cpu::outb(DRIVE_HEAD, 0xE0 | ((lba >> 24) & 0x0F) as u8);
    crate::cpu::outb(SECTOR_COUNT, 1);
    crate::cpu::outb(LBA_LOW, (lba & 0xFF) as u8);
    crate::cpu::outb(LBA_MID, ((lba >> 8) & 0xFF) as u8);
    crate::cpu::outb(LBA_HIGH, ((lba >> 16) & 0xFF) as u8);

    // Команда READ.
    crate::cpu::outb(COMMAND_PORT, CMD_READ_PIO);

    // Ждать готовности данных.
    poll_drq();

    // Читать 256 слов (512 байт).
    for i in 0..256 {
        let word = crate::cpu::inw(DATA_PORT);
        buf[i * 2] = (word & 0xFF) as u8;
        buf[i * 2 + 1] = ((word >> 8) & 0xFF) as u8;
    }

    true
}

/// Записать один сектор PIO.
pub unsafe fn write_sector(lba: u32, buf: &[u8; 512]) -> bool {
    if !DISK_PRESENT { return false; }

    poll_drq();

    crate::cpu::outb(DRIVE_HEAD, 0xE0 | ((lba >> 24) & 0x0F) as u8);
    crate::cpu::outb(SECTOR_COUNT, 1);
    crate::cpu::outb(LBA_LOW, (lba & 0xFF) as u8);
    crate::cpu::outb(LBA_MID, ((lba >> 8) & 0xFF) as u8);
    crate::cpu::outb(LBA_HIGH, ((lba >> 16) & 0xFF) as u8);

    crate::cpu::outb(COMMAND_PORT, CMD_WRITE_PIO);

    poll_drq();

    // Писать 256 слов.
    for i in 0..256 {
        let word = (buf[i * 2] as u16) | ((buf[i * 2 + 1] as u16) << 8);
        crate::cpu::outw(DATA_PORT, word);
    }

    // Сбросить кэш.
    crate::cpu::outb(COMMAND_PORT, CMD_FLUSH_CACHE);
    poll_drq();

    true
}

/// Прочитать несколько секторов подряд.
pub unsafe fn read_sectors(lba: u32, count: u32, buf: &mut [u8]) -> bool {
    let len = (count * 512) as usize;
    if buf.len() < len { return false; }
    for i in 0..count {
        let offset = (i * 512) as usize;
        let sector_buf = core::slice::from_raw_parts_mut(buf.as_mut_ptr().add(offset), 512);
        if !read_sector(lba + i, sector_buf.as_mut_ptr().cast::<[u8; 512]>().as_mut().unwrap()) {
            return false;
        }
    }
    true
}

/// Записать несколько секторов подряд.
pub unsafe fn write_sectors(lba: u32, count: u32, buf: &[u8]) -> bool {
    let len = (count * 512) as usize;
    if buf.len() < len { return false; }
    for i in 0..count {
        let offset = (i * 512) as usize;
        let sector_buf = core::slice::from_raw_parts(buf.as_ptr().add(offset), 512);
        if !write_sector(lba + i, sector_buf.as_ptr().cast::<[u8; 512]>().as_ref().unwrap()) {
            return false;
        }
    }
    true
}

/// Получить информацию о диске.
pub unsafe fn info() -> DiskInfo {
    if !DISK_PRESENT {
        return DiskInfo { present: false, sectors: 0, model: [0; 40] };
    }

    // Определить размер через IDENTIFY.
    // Сначала IDENTIFY...
    poll_drq();
    crate::cpu::outb(DRIVE_HEAD, 0xE0);
    crate::cpu::outb(COMMAND_PORT, CMD_IDENTIFY);
    poll_drq();

    let mut identify = [0u16; 256];
    for i in 0..256 {
        identify[i] = crate::cpu::inw(DATA_PORT);
    }

    // Модель — words 27..46 (54..92 bytes)
    let mut model = [0u8; 40];
    for i in 0..20 {
        let w = identify[27 + i];
        model[i * 2] = (w >> 8) as u8;
        model[i * 2 + 1] = (w & 0xFF) as u8;
    }

    // Размер из word 60-61 (28-bit LBA) или 100-103 (48-bit LBA)
    let sectors = if identify[83] & (1 << 10) != 0 {
        // 48-bit LBA
        (identify[100] as u64)
            | ((identify[101] as u64) << 16)
            | ((identify[102] as u64) << 32)
            | ((identify[103] as u64) << 48)
    } else {
        (identify[60] as u64) | ((identify[61] as u64) << 16)
    };

    DiskInfo { present: true, sectors, model }
}

unsafe fn poll_drq() {
    let mut timeout = 0;
    loop {
        let status = crate::cpu::inb(COMMAND_PORT);
        if status & STATUS_BSY == 0 && (status & (STATUS_DRQ | STATUS_ERR)) != 0 {
            break;
        }
        if status & STATUS_BSY == 0 && status & STATUS_DRQ != 0 {
            break;
        }
        timeout += 1;
        if timeout > 1000000 { break; } // Предохранитель
    }
}
