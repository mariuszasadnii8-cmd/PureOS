//! Система конфигурации PureOS
//! Позволяет кастомизировать различные параметры системы

use crate::terminal;

/// Конфигурация системы
pub struct SystemConfig {
    pub font_scale: u32,
    pub terminal_colors: TerminalColors,
    pub shell_prompt: [u8; 64],
    pub max_processes: usize,
    pub ephemeral_layer_size: u64,
    pub enable_graphics: bool,
    pub boot_delay_ms: u64,
}

impl Clone for SystemConfig {
    fn clone(&self) -> Self {
        *self
    }
}

impl Copy for SystemConfig {}

/// Цвета терминала
#[derive(Clone, Copy)]
pub struct TerminalColors {
    pub foreground_r: u8,
    pub foreground_g: u8,
    pub foreground_b: u8,
    pub background_r: u8,
    pub background_g: u8,
    pub background_b: u8,
}

impl TerminalColors {
    pub const fn default() -> Self {
        Self {
            foreground_r: 255,
            foreground_g: 255,
            foreground_b: 255,
            background_r: 0,
            background_g: 0,
            background_b: 0,
        }
    }
    
    pub const fn light() -> Self {
        Self {
            foreground_r: 0,
            foreground_g: 0,
            foreground_b: 0,
            background_r: 255,
            background_g: 255,
            background_b: 255,
        }
    }
}

impl SystemConfig {
    pub const fn default() -> Self {
        Self {
            font_scale: 1,
            terminal_colors: TerminalColors::default(),
            shell_prompt: *b"pureos$ \0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            max_processes: 64,
            ephemeral_layer_size: 16 * 1024 * 1024,
            enable_graphics: true,
            boot_delay_ms: 0,
        }
    }
}

/// Глобальная конфигурация системы
static mut CONFIG: SystemConfig = SystemConfig::default();

/// Получить текущую конфигурацию
pub unsafe fn get_config() -> SystemConfig {
    CONFIG.clone()
}

/// Установить конфигурацию
pub unsafe fn set_config(config: SystemConfig) {
    CONFIG = config;
}

/// Установить масштаб шрифта
pub unsafe fn set_font_scale(scale: u32) {
    CONFIG.font_scale = scale;
}

/// Получить масштаб шрифта
pub unsafe fn get_font_scale() -> u32 {
    CONFIG.font_scale
}

/// Установить цвета терминала
pub unsafe fn set_terminal_colors(colors: TerminalColors) {
    CONFIG.terminal_colors = colors;
}

/// Получить цвета терминала
pub unsafe fn get_terminal_colors() -> TerminalColors {
    CONFIG.terminal_colors
}

/// Установить промпт оболочки
pub unsafe fn set_shell_prompt(prompt: &[u8]) {
    let mut i = 0;
    while i < prompt.len() && i < 63 {
        CONFIG.shell_prompt[i] = prompt[i];
        i += 1;
    }
    CONFIG.shell_prompt[i] = 0;
}

/// Получить промпт оболочки
pub unsafe fn get_shell_prompt() -> &'static [u8] {
    let mut len = 0;
    while len < 64 && CONFIG.shell_prompt[len] != 0 {
        len += 1;
    }
    &CONFIG.shell_prompt[..len]
}

/// Установить максимальное количество процессов
pub unsafe fn set_max_processes(count: usize) {
    if count <= 64 && count >= 1 {
        CONFIG.max_processes = count;
    }
}

/// Получить максимальное количество процессов
pub unsafe fn get_max_processes() -> usize {
    CONFIG.max_processes
}

/// Установить размер эфемерного слоя
pub unsafe fn set_ephemeral_layer_size(size: u64) {
    if size >= 1024 * 1024 && size <= 1024 * 1024 * 1024 {
        CONFIG.ephemeral_layer_size = size;
    }
}

/// Получить размер эфемерного слоя
pub unsafe fn get_ephemeral_layer_size() -> u64 {
    CONFIG.ephemeral_layer_size
}

/// Включить/выключить графику
pub unsafe fn set_graphics_enabled(enabled: bool) {
    CONFIG.enable_graphics = enabled;
}

/// Проверить включена ли графика
pub unsafe fn is_graphics_enabled() -> bool {
    CONFIG.enable_graphics
}

/// Установить задержку загрузки
pub unsafe fn set_boot_delay(delay_ms: u64) {
    CONFIG.boot_delay_ms = delay_ms;
}

/// Получить задержку загрузки
pub unsafe fn get_boot_delay() -> u64 {
    CONFIG.boot_delay_ms
}

/// Показать текущую конфигурацию
pub unsafe fn show_config() {
    terminal::write(b"\n=== PureOS Configuration ===\n\n");
    
    terminal::write(b"Font Scale: ");
    terminal::write_num(CONFIG.font_scale as u64);
    terminal::write(b"\n");
    
    terminal::write(b"Terminal Colors: RGB(");
    terminal::write_num(CONFIG.terminal_colors.foreground_r as u64);
    terminal::write(b", ");
    terminal::write_num(CONFIG.terminal_colors.foreground_g as u64);
    terminal::write(b", ");
    terminal::write_num(CONFIG.terminal_colors.foreground_b as u64);
    terminal::write(b") / RGB(");
    terminal::write_num(CONFIG.terminal_colors.background_r as u64);
    terminal::write(b", ");
    terminal::write_num(CONFIG.terminal_colors.background_g as u64);
    terminal::write(b", ");
    terminal::write_num(CONFIG.terminal_colors.background_b as u64);
    terminal::write(b")\n");
    
    terminal::write(b"Shell Prompt: ");
    let prompt = get_shell_prompt();
    terminal::write(prompt);
    terminal::write(b"\n");
    
    terminal::write(b"Max Processes: ");
    terminal::write_num(CONFIG.max_processes as u64);
    terminal::write(b"\n");
    
    terminal::write(b"Ephemeral Layer Size: ");
    terminal::write_num(CONFIG.ephemeral_layer_size / (1024 * 1024));
    terminal::write(b" MB\n");
    
    terminal::write(b"Graphics: ");
    if CONFIG.enable_graphics {
        terminal::write(b"Enabled\n");
    } else {
        terminal::write(b"Disabled\n");
    }
    
    terminal::write(b"Boot Delay: ");
    terminal::write_num(CONFIG.boot_delay_ms);
    terminal::write(b" ms\n\n");
}

/// Сбросить конфигурацию на значения по умолчанию
pub unsafe fn reset_config() {
    CONFIG = SystemConfig::default();
}

/// Применить пресет конфигурации
pub unsafe fn apply_preset(preset: &[u8]) {
    match preset {
        b"default" => reset_config(),
        b"minimal" => {
            CONFIG.font_scale = 1;
            CONFIG.max_processes = 16;
            CONFIG.ephemeral_layer_size = 8 * 1024 * 1024;
            CONFIG.enable_graphics = false;
        }
        b"performance" => {
            CONFIG.font_scale = 2;
            CONFIG.max_processes = 64;
            CONFIG.ephemeral_layer_size = 32 * 1024 * 1024;
            CONFIG.enable_graphics = true;
        }
        b"light" => {
            CONFIG.terminal_colors = TerminalColors::light();
        }
        b"dark" => {
            CONFIG.terminal_colors = TerminalColors::default();
        }
        _ => {
            terminal::write(b"Unknown preset: ");
            terminal::write(preset);
            terminal::write(b"\n");
        }
    }
}

/// Сохранить конфигурацию (заглушка - нужно реализовать сохранение на диск)
pub unsafe fn save_config() -> bool {
    // TODO: Реальное сохранение конфигурации на диск
    terminal::write(b"Configuration saved (not yet implemented)\n");
    true
}

/// Загрузить конфигурацию (заглушка - нужно реализовать загрузку с диска)
pub unsafe fn load_config() -> bool {
    // TODO: Реальная загрузка конфигурации с диска
    terminal::write(b"Configuration loaded (not yet implemented)\n");
    true
}
