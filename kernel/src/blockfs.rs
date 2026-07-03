//! Блочная файловая система PureOS (persistent поверх ATA/диска).
//!
//! Простая файловая система поверх блочного устройства.
//! Формат:
//!   - Сектор 0: суперблок (magic, размеры, корень)
//!   - Сектор 1: bitmap inode (какие inode заняты)
//!   - Сектор 2: bitmap блоков (какие блоки данных заняты)
//!   - Секторы 3..(3+INODE_TABLE_SECTORS): таблица inode
//!   - Остальное: блоки данных
//!
//! Сильно упрощено: flat-директория, maximum 64 файла, фикс. размер блока = 512.
//! Синхронизируется с ramfs при монтировании.

use crate::ata;

const SUPER_MAGIC: u32 = 0x50555245; // "PURE"
const SUPER_SECTOR: u32 = 0;
const INODE_BITMAP_SECTOR: u32 = 1;
const BLOCK_BITMAP_SECTOR: u32 = 2;
const INODE_TABLE_START: u32 = 3;
const INODE_TABLE_SECTORS: u32 = 4;  // 4*512 = 2048 bytes, 64 inode по 32 байта
const DATA_START_SECTOR: u32 = INODE_TABLE_START + INODE_TABLE_SECTORS;
const MAX_INODES: u32 = 64;
const BLOCKS_PER_SECTOR: u32 = 1; // block = sector = 512 bytes
const BLOCK_SIZE: u32 = 512;

const INODE_FREE: u8 = 0;
const INODE_FILE: u8 = 1;
const INODE_DIR: u8 = 2;

#[repr(C)]
struct Superblock {
    magic: u32,
    version: u32,
    total_inodes: u32,
    total_blocks: u32,
    inode_table_size: u32,
    data_start: u32,
    _pad: [u8; 488],
}

#[repr(C)]
#[derive(Copy, Clone)]
struct Inode {
    kind: u8,
    name_len: u8,
    name: [u8; 28],
    data_block: u32,
    data_size: u32,
    _pad: [u8; 2],
}

static mut MOUNTED: bool = false;

/// Инициализация блочной ФС.
pub unsafe fn init() {
    if MOUNTED { return; }
    ata::init();
    if !ata::info().present {
        crate::console::serial_puts(b"[BLKFS] no disk present, skipping\n");
        return;
    }
    // Попробовать прочитать суперблок.
    let mut sb_buf = [0u8; 512];
    if !ata::read_sector(SUPER_SECTOR, &mut sb_buf) {
        crate::console::serial_puts(b"[BLKFS] cannot read superblock\n");
        return;
    }
    let sb: &Superblock = &*(sb_buf.as_ptr() as *const Superblock);
    if sb.magic != SUPER_MAGIC {
        crate::console::serial_puts(b"[BLKFS] no filesystem, formatting...\n");
        format();
    } else {
        crate::console::serial_puts(b"[BLKFS] valid superblock, mounting\n");
    }
    MOUNTED = true;
}

/// Форматирование диска.
unsafe fn format() {
    // Суперблок
    let sb = Superblock {
        magic: SUPER_MAGIC,
        version: 1,
        total_inodes: MAX_INODES,
        total_blocks: 65536,
        inode_table_size: INODE_TABLE_SECTORS,
        data_start: DATA_START_SECTOR,
        _pad: [0; 488],
    };
    let sb_bytes = core::slice::from_raw_parts(
        &sb as *const Superblock as *const u8,
        core::mem::size_of::<Superblock>(),
    );
    write_sector_raw(SUPER_SECTOR, sb_bytes);

    // Inode bitmap (1-й занят корнем), остальные свободны
    let mut bitmap = [0u8; 512];
    bitmap[0] = 0x01; // inode 0: корень
    bitmap[1] = 0x01; // inode 1: /dev
    write_sector_raw(INODE_BITMAP_SECTOR, &bitmap);

    // Block bitmap — первые несколько блоков заняты метаданными.
    let mut block_bitmap = [0u8; 512];
    for b in 0..DATA_START_SECTOR as usize {
        block_bitmap[b >> 3] |= 1 << (b & 7);
    }
    write_sector_raw(BLOCK_BITMAP_SECTOR, &block_bitmap);

    // Корневой inode (0)
    let mut root_inode = Inode {
        kind: INODE_DIR,
        name_len: 1,
        name: [0; 28],
        data_block: 0,
        data_size: 0,
        _pad: [0; 2],
    };
    root_inode.name[0] = b'/';
    write_inode(0, &root_inode);

    // Inode 1: /dev
    let mut dev_inode = Inode {
        kind: INODE_DIR,
        name_len: 3,
        name: [0; 28],
        data_block: 0,
        data_size: 0,
        _pad: [0; 2],
    };
    dev_inode.name[0..3].copy_from_slice(b"dev");
    write_inode(1, &dev_inode);

    crate::console::serial_puts(b"[BLKFS] format done\n");
}

/// Прочитать inode.
unsafe fn read_inode(num: u32) -> Option<Inode> {
    if num >= MAX_INODES { return None; }
    let inode_sector = INODE_TABLE_START + (num * core::mem::size_of::<Inode>() as u32) / 512;
    let inode_off = (num * core::mem::size_of::<Inode>() as u32) % 512;
    let mut buf = [0u8; 512];
    if !ata::read_sector(inode_sector, &mut buf) { return None; }
    let off = inode_off as usize;
    let ptr = buf.as_ptr().add(off) as *const Inode;
    let inode = core::ptr::read_volatile(ptr);
    if inode.kind == INODE_FREE { return None; }
    Some(inode)
}

/// Записать inode.
unsafe fn write_inode(num: u32, inode: &Inode) -> bool {
    if num >= MAX_INODES { return false; }
    let inode_sector = INODE_TABLE_START + (num * core::mem::size_of::<Inode>() as u32) / 512;
    let inode_off = (num * core::mem::size_of::<Inode>() as u32) % 512;
    let mut buf = [0u8; 512];
    let _ = ata::read_sector(inode_sector, &mut buf);
    let off = inode_off as usize;
    let ptr = buf.as_mut_ptr().add(off) as *mut Inode;
    core::ptr::write_volatile(ptr, *inode);
    write_sector_raw(inode_sector, &buf);
    true
}

/// Записать сектор из среза.
unsafe fn write_sector_raw(sector: u32, data: &[u8]) {
    let mut buf = [0u8; 512];
    let n = data.len().min(512);
    for i in 0..n { buf[i] = data[i]; }
    ata::write_sector(sector, &buf);
}

/// Прочитать блок данных.
pub unsafe fn read_block(block_num: u32, buf: &mut [u8; 512]) -> bool {
    ata::read_sector(DATA_START_SECTOR + block_num, buf)
}

/// Записать блок данных.
pub unsafe fn write_block(block_num: u32, buf: &[u8; 512]) -> bool {
    ata::write_sector(DATA_START_SECTOR + block_num, buf)
}

/// Проверить, смонтирована ли ФС.
pub fn is_mounted() -> bool {
    unsafe { MOUNTED }
}
