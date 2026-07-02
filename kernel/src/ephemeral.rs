//! Логика эфемерных веток (слоев) памяти.
//! Каждый процесс работает в изолированном эфемерном слое.
//!
//! Пока это концептуальный тип: реальный учёт слоёв ведёт `syscall.rs` в полях
//! PCB (`layer_base`/`layer_size`/`next_free`). `EphemeralLayer` зарезервирован
//! под будущую рекламацию фреймов на `exit` (веха №2).
#![allow(dead_code)]

/// Эфемерный слой памяти процесса
pub struct EphemeralLayer {
    pub base: u64,
    pub size: u64,
    pub is_readonly: bool,
}

impl EphemeralLayer {
    pub const fn new(base: u64, size: u64, is_readonly: bool) -> Self {
        Self {
            base,
            size,
            is_readonly,
        }
    }

    pub fn contains(&self, addr: u64) -> bool {
        addr >= self.base && addr < self.base + self.size
    }
}
