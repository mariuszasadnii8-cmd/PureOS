//! Система конфигурации PureOS
//! Позволяет кастомизировать различные параметры системы

use crate::terminal;

/// Конфигурация системы
pub struct SystemConfig {
    pub font_scale: u32,
    pub selected_font: u32, // индекс в FontId (0..7)
    pub terminal_colors: TerminalColors,
    pub shell_prompt: [u8; 64],
    pub max_processes: usize,
    pub ephemeral_layer_size: u64,
    pub enable_graphics: bool,
    pub boot_delay_ms: u64,
    pub mouse_sensitivity: u32, // 1..9
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
            selected_font: 0, // Compact
            terminal_colors: TerminalColors::default(),
            shell_prompt: *b"pureos$ \0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            max_processes: 64,
            ephemeral_layer_size: 16 * 1024 * 1024,
            enable_graphics: true,
            boot_delay_ms: 0,
            mouse_sensitivity: 5,
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

/// Установить выбранный шрифт (индекс 0..7)
pub unsafe fn set_selected_font(idx: u32) {
    if idx <= 7 {
        CONFIG.selected_font = idx;
    }
}

/// Получить выбранный шрифт
pub unsafe fn get_selected_font() -> u32 {
    CONFIG.selected_font
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

/// Установить чувствительность мыши (1-9)
pub unsafe fn set_mouse_sensitivity(val: u32) {
    if val >= 1 && val <= 9 {
        CONFIG.mouse_sensitivity = val;
    }
}

/// Получить чувствительность мыши
pub unsafe fn get_mouse_sensitivity() -> u32 {
    CONFIG.mouse_sensitivity
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
    
    terminal::write(b"Screen Resolution: ");
    terminal::write_num(crate::framebuffer::width() as u64);
    terminal::write(b"x");
    terminal::write_num(crate::framebuffer::height() as u64);
    terminal::write(b" @32bpp\n");
    
    terminal::write(b"Font Selection: ");
    let font_names: [&[u8]; 8] = [b"compact", b"bold", b"italic", b"serif", b"outline", b"tall", b"vga", b"wide"];
    let idx = if CONFIG.selected_font < 8 { CONFIG.selected_font as usize } else { 0 };
    terminal::write(font_names[idx]);
    terminal::write(b" (scale: ");
    terminal::write_num(CONFIG.font_scale as u64);
    terminal::write(b")\n");
    
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

/// Сохранить конфигурацию в файловую систему (ramfs).
pub unsafe fn save_config() -> bool {
    // Сериализовать CONFIG в плоский буфер: header + fields.
    let mut buf = [0u8; 128];
    buf[0] = b'P'; buf[1] = b'C'; buf[2] = b'F'; buf[3] = 1; // magic "PCF" v1
    let mut off = 4;
    // font_scale (u32)
    buf[off..off+4].copy_from_slice(&CONFIG.font_scale.to_le_bytes()); off += 4;
    // selected_font (u32)
    buf[off..off+4].copy_from_slice(&CONFIG.selected_font.to_le_bytes()); off += 4;
    // terminal_colors (6 x u8)
    buf[off] = CONFIG.terminal_colors.foreground_r; off += 1;
    buf[off] = CONFIG.terminal_colors.foreground_g; off += 1;
    buf[off] = CONFIG.terminal_colors.foreground_b; off += 1;
    buf[off] = CONFIG.terminal_colors.background_r; off += 1;
    buf[off] = CONFIG.terminal_colors.background_g; off += 1;
    buf[off] = CONFIG.terminal_colors.background_b; off += 1;
    // shell_prompt ([u8; 64])
    let prompt_ptr = core::ptr::addr_of!(CONFIG.shell_prompt) as *const u8;
    for i in 0..64 { buf[off + i] = *prompt_ptr.add(i); } off += 64;
    // max_processes (usize -> u64)
    buf[off..off+8].copy_from_slice(&(CONFIG.max_processes as u64).to_le_bytes()); off += 8;
    // ephemeral_layer_size (u64)
    buf[off..off+8].copy_from_slice(&CONFIG.ephemeral_layer_size.to_le_bytes()); off += 8;
    // enable_graphics (u8)
    buf[off] = if CONFIG.enable_graphics { 1 } else { 0 }; off += 1;
    // boot_delay_ms (u64)
    buf[off..off+8].copy_from_slice(&CONFIG.boot_delay_ms.to_le_bytes()); off += 8;

    // mouse_sensitivity (u32)
    buf[off..off+4].copy_from_slice(&CONFIG.mouse_sensitivity.to_le_bytes()); off += 4;

    let data = &buf[..off];
    // Создать /etc/pureos.conf
    let path = match crate::fs::resolve(b"/etc/pureos.conf") {
        Some(n) if crate::fs::kind(n) == crate::fs::Kind::File => n,
        _ => {
            let etc = match crate::fs::resolve(b"/etc") {
                Some(n) if crate::fs::kind(n) == crate::fs::Kind::Dir => n,
                _ => match crate::fs::mkdir(crate::fs::ROOT, b"etc") {
                    Some(n) => n,
                    None => { terminal::write(b"config: cannot create /etc\n"); return false; }
                },
            };
            match crate::fs::create_file(etc, b"pureos.conf") {
                Some(n) => n,
                None => { terminal::write(b"config: cannot create /etc/pureos.conf\n"); return false; }
            }
        }
    };
    crate::fs::write(path, data);
    terminal::write(b"Configuration saved to /etc/pureos.conf\n");
    true
}

/// Загрузить конфигурацию из файловой системы.
pub unsafe fn load_config() -> bool {
    let path = match crate::fs::resolve(b"/etc/pureos.conf") {
        Some(n) if crate::fs::kind(n) == crate::fs::Kind::File => n,
        _ => { terminal::write(b"config: /etc/pureos.conf not found\n"); return false; }
    };
    let data = crate::fs::read(path);
    if data.len() < 4 || data[0] != b'P' || data[1] != b'C' || data[2] != b'F' || data[3] != 1 {
        terminal::write(b"config: bad format\n"); return false;
    }
    if data.len() < 104 {
        terminal::write(b"config: truncated\n"); return false;
    }
    let mut off = 4;
    CONFIG = SystemConfig::default();
    CONFIG.font_scale = u32::from_le_bytes([data[off], data[off+1], data[off+2], data[off+3]]); off += 4;
    CONFIG.selected_font = u32::from_le_bytes([data[off], data[off+1], data[off+2], data[off+3]]); off += 4;
    CONFIG.terminal_colors.foreground_r = data[off]; off += 1;
    CONFIG.terminal_colors.foreground_g = data[off]; off += 1;
    CONFIG.terminal_colors.foreground_b = data[off]; off += 1;
    CONFIG.terminal_colors.background_r = data[off]; off += 1;
    CONFIG.terminal_colors.background_g = data[off]; off += 1;
    CONFIG.terminal_colors.background_b = data[off]; off += 1;
    for i in 0..64 { CONFIG.shell_prompt[i] = data[off + i]; } off += 64;
    CONFIG.max_processes = u64::from_le_bytes([data[off], data[off+1], data[off+2], data[off+3],
                                                data[off+4], data[off+5], data[off+6], data[off+7]]) as usize; off += 8;
    CONFIG.ephemeral_layer_size = u64::from_le_bytes([data[off], data[off+1], data[off+2], data[off+3],
                                                      data[off+4], data[off+5], data[off+6], data[off+7]]); off += 8;
    CONFIG.enable_graphics = data[off] != 0; off += 1;
    CONFIG.boot_delay_ms = u64::from_le_bytes([data[off], data[off+1], data[off+2], data[off+3],
                                                data[off+4], data[off+5], data[off+6], data[off+7]]); off += 8;
    CONFIG.mouse_sensitivity = u32::from_le_bytes([data[off], data[off+1], data[off+2], data[off+3]]);
    terminal::write(b"Configuration loaded from /etc/pureos.conf\n");
    true
}
