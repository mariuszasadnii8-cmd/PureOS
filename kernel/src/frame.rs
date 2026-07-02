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

/// Инициализировать пул диапазоном `[base, base + size)`. База выравнивается
/// вверх до границы фрейма. Пустой/нулевой пул допустим — `alloc_frame` тогда
/// сразу возвращает `None`.
pub unsafe fn init(base: u64, size: u64) {
    if size == 0 {
        POOL_NEXT = 0;
        POOL_END = 0;
        return;
    }
    let aligned = (base + FRAME_SIZE - 1) & !(FRAME_SIZE - 1);
    POOL_NEXT = aligned;
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
