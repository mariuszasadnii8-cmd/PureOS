//! Физический frame-allocator ядра.
//!
//! Работает поверх непрерывного пула физпамяти, который UEFI-загрузчик
//! зарезервировал через AllocatePages и передал в `PureBootInfo`. Аллокатор
//! bump-only: освобождение отдельных фреймов не поддерживается на этой вехе —
//! это согласуется с философией эфемерных слоёв (слой «испаряется» целиком).
//!
//! Инвариант: физпамять отображена идентично (phys == virt) в ядре, поэтому
//! адрес фрейма одновременно и физический, и пригодный для разыменования из
//! ядра указатель. То же допущение действует в `cpu::map_page`.

use core::ptr::write_bytes;

pub const FRAME_SIZE: u64 = 4096;

// Курсор пула и его верхняя граница (эксклюзивно). Замораживаются один раз в
// `init` и далее меняется только `POOL_NEXT` при выдаче фреймов.
static mut POOL_NEXT: u64 = 0;
static mut POOL_END: u64 = 0;
// База пула (выровненная) — замораживается в `init`, нужна для статистики.
static mut POOL_BASE: u64 = 0;

/// Сводная статистика пула фреймов (в байтах и во фреймах).
#[derive(Copy, Clone)]
pub struct FrameStats {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub total_frames: u64,
    pub used_frames: u64,
    pub free_frames: u64,
}

/// Снять текущую статистику пула. Bump-аллокатор: used = next - base.
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

/// Инициализировать пул диапазоном `[base, base + size)`. База выравнивается
/// вверх до границы фрейма. Пустой/нулевой пул допустим — `alloc_frame` тогда
/// сразу возвращает `None`.
pub unsafe fn init(base: u64, size: u64) {
    if size == 0 {
        POOL_NEXT = 0;
        POOL_END = 0;
        POOL_BASE = 0;
        return;
    }
    let aligned = (base + FRAME_SIZE - 1) & !(FRAME_SIZE - 1);
    POOL_NEXT = aligned;
    POOL_BASE = aligned;
    POOL_END = base + size;
}

/// Выделить один физический фрейм 4 KiB, занулив его перед выдачей.
/// Зануление обязательно: свежие таблицы страниц и пользовательская память не
/// должны содержать чужих данных. Возвращает физ. адрес фрейма или `None` при
/// исчерпании пула.
pub unsafe fn alloc_frame() -> Option<u64> {
    let next = POOL_NEXT;
    if next == 0 || next + FRAME_SIZE > POOL_END {
        return None;
    }
    POOL_NEXT = next + FRAME_SIZE;
    write_bytes(next as *mut u8, 0, FRAME_SIZE as usize);
    Some(next)
}
