//! Физический frame-allocator ядра.
//!
//! Работает поверх непрерывного пула физпамяти, который UEFI-загрузчик
//! зарезервировал через AllocatePages и передал в `PureBootInfo`.
//! Bump-allocator с поддержкой возврата фреймов через singly-linked free list.
//! Свободные фреймы хранят указатель на следующий свободный в первых 8 байтах.
//!
//! Инвариант: физпамять отображена идентично (phys == virt) в ядре, поэтому
//! адрес фрейма одновременно и физический, и пригодный для разыменования из
//! ядра указатель. То же допущение действует в `cpu::map_page`.

use core::ptr::{write_bytes, read_volatile, write_volatile};

pub const FRAME_SIZE: u64 = 4096;

static mut POOL_NEXT: u64 = 0;
static mut POOL_END: u64 = 0;
static mut POOL_BASE: u64 = 0;
static mut FREE_LIST_HEAD: u64 = 0;

#[derive(Copy, Clone)]
pub struct FrameStats {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub total_frames: u64,
    pub used_frames: u64,
    pub free_frames: u64,
}

pub fn stats() -> FrameStats {
    unsafe {
        let base = POOL_BASE;
        let next = POOL_NEXT;
        let end = POOL_END;
        if base == 0 || end <= base {
            return FrameStats {
                total_bytes: 0, used_bytes: 0, free_bytes: 0,
                total_frames: 0, used_frames: 0, free_frames: 0,
            };
        }
        let total = end - base;
        let used = next.saturating_sub(base);
        let free = total - used;
        FrameStats {
            total_bytes: total,
            used_bytes: used,
            free_bytes: free,
            total_frames: total / FRAME_SIZE,
            used_frames: used / FRAME_SIZE,
            free_frames: free / FRAME_SIZE,
        }
    }
}

/// Реальная статистика с учётом free-list.
pub fn real_stats() -> FrameStats {
    unsafe {
        let s = stats();
        let mut free = s.free_frames;
        let mut f = FREE_LIST_HEAD;
        while f != 0 {
            free += 1;
            f = read_volatile(f as *const u64);
        }
        FrameStats {
            total_bytes: s.total_bytes,
            used_bytes: (s.total_frames - free) * FRAME_SIZE,
            free_bytes: free * FRAME_SIZE,
            total_frames: s.total_frames,
            used_frames: s.total_frames - free,
            free_frames: free,
        }
    }
}

pub unsafe fn init(base: u64, size: u64) {
    if size == 0 {
        POOL_NEXT = 0;
        POOL_END = 0;
        POOL_BASE = 0;
        FREE_LIST_HEAD = 0;
        return;
    }
    let aligned = (base + FRAME_SIZE - 1) & !(FRAME_SIZE - 1);
    POOL_NEXT = aligned;
    POOL_BASE = aligned;
    POOL_END = base + size;
    FREE_LIST_HEAD = 0;
}

/// Вернуть фрейм обратно в пул (через free-list).
pub unsafe fn free_frame(phys: u64) {
    if phys == 0 || (phys & 0xFFF) != 0 { return; }
    write_volatile(phys as *mut u64, FREE_LIST_HEAD);
    FREE_LIST_HEAD = phys;
}

/// Выделить один физический фрейм 4 KiB, занулив его перед выдачей.
/// Сначала проверяет free-list, затем bump.
pub unsafe fn alloc_frame() -> Option<u64> {
    if FREE_LIST_HEAD != 0 {
        let frame = FREE_LIST_HEAD;
        FREE_LIST_HEAD = read_volatile(frame as *const u64);
        write_bytes(frame as *mut u8, 0, FRAME_SIZE as usize);
        return Some(frame);
    }
    let next = POOL_NEXT;
    if next == 0 || next + FRAME_SIZE > POOL_END {
        return None;
    }
    POOL_NEXT = next + FRAME_SIZE;
    write_bytes(next as *mut u8, 0, FRAME_SIZE as usize);
    Some(next)
}
