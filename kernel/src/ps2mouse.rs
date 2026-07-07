//! Драйвер PS/2 мыши/тачпада через i8042.
//!
//! Читает 3-байтные пакеты с auxiliary порта (STATUS_AUX) и передаёт
//! дельты в usb::hid (общий курсор). Работает без аппаратных прерываний.
//!
//! Инициализация:
//!   1. Прочитать/записать Controller Config Byte (вкл. второй порт)
//!   2. Включить второй порт (0xA8 → 0x64)
//!   3. Отправить 0xF6 (Set Defaults) в мышь
//!   4. Отправить 0xF4 (Enable Data Reporting)
//!   5. После ответа 0xFA (ACK) мышь шлёт 3-байтные пакеты ~40-80 раз/с

use crate::cpu;

const PS2_DATA: u16 = 0x60;
const PS2_CMD:  u16 = 0x64;

const STATUS_OBF: u8 = 0x01;
const STATUS_IBF: u8 = 0x02; // input buffer full — не писать, пока установлен
const STATUS_AUX: u8 = 0x20; // байт со второго порта (мышь)

const ACK: u8 = 0xFA;
const CMD_RD_CONFIG: u8 = 0x20;
const CMD_WR_CONFIG: u8 = 0x60;
const CMD_ENABLE_AUX: u8 = 0xA8;
const CMD_SEND_TO_AUX: u8 = 0xD4;
const CMD_SET_DEFAULTS: u8 = 0xF6;
const CMD_ENABLE_REPORT: u8 = 0xF4;

static mut INIT_DONE: bool = false;
static mut INIT_FAILED: bool = false;
static mut RETRY_TICKS: u32 = 0;
const RETRY_INTERVAL: u32 = 300; // ~300 polls между retry

/// Включён/выключен мыши (выключается командой `mouse off`).
static mut MOUSE_ENABLED: bool = true;

// Состояние декодирования пакета
static mut PACKET_BUF: [u8; 3] = [0; 3];
static mut PACKET_POS: usize = 0;
static mut MOUSE_X: i32 = 0;
static mut MOUSE_Y: i32 = 0;
static mut MOUSE_BUTTONS: u8 = 0;

// Ждать, пока контроллер будет готов принять команду (IBF = 0)
unsafe fn wait_write() {
    for _ in 0..1000 {
        if cpu::inb(PS2_CMD) & STATUS_IBF == 0 { return; }
        core::hint::spin_loop();
    }
}

// Ждать, пока контроллер будет готов отдать данные (OBF = 1)
unsafe fn wait_read() -> bool {
    for _ in 0..1000 {
        if cpu::inb(PS2_CMD) & STATUS_OBF != 0 { return true; }
        core::hint::spin_loop();
    }
    false
}

// Отправить команду мышке через 0xD4 и дождаться ACK
unsafe fn send_to_mouse(cmd: u8) -> bool {
    wait_write();
    cpu::outb(PS2_CMD, CMD_SEND_TO_AUX);
    wait_write();
    cpu::outb(PS2_DATA, cmd);
    // Ждать ACK
    for _ in 0..500 {
        if wait_read() {
            let byte = cpu::inb(PS2_DATA);
            if byte == ACK { return true; }
        }
        core::hint::spin_loop();
    }
    false
}

/// Инициализировать PS/2 мышь (полный протокол).
pub unsafe fn init() {
    if INIT_DONE || INIT_FAILED { return; }

    // Сбросить накопившиеся байты
    drain();

    // 1. Прочитать Controller Config Byte (cmd 0x20)
    wait_write();
    cpu::outb(PS2_CMD, CMD_RD_CONFIG);
    if !wait_read() { INIT_FAILED = true; return; }
    let mut config = cpu::inb(PS2_DATA);

    // 2. Установить биты: bit 1 (enable aux interrupt — необязательно для polling)
    //    и bit 6 (enable second PS/2 port clock)
    config |= 0x04; // bit 2 = enable aux port clock
    wait_write();
    cpu::outb(PS2_CMD, CMD_WR_CONFIG);
    wait_write();
    cpu::outb(PS2_DATA, config);

    // 3. Включить второй порт (aux)
    wait_write();
    cpu::outb(PS2_CMD, CMD_ENABLE_AUX);

    // 4. Отправить Set Defaults (0xF6) — сброс в 3-байтный режим, 100 counts/mm
    let defaults_ok = send_to_mouse(CMD_SET_DEFAULTS);

    // 5. Отправить Enable Data Reporting (0xF4)
    let enable_ok = send_to_mouse(CMD_ENABLE_REPORT);

    // Если оба раза не ответили ACK — мыши нет
    if !defaults_ok && !enable_ok {
        INIT_FAILED = true;
        RETRY_TICKS = 0;
        return;
    }

    drain();

    INIT_DONE = true;
    PACKET_POS = 0;

    // Сбросить позицию на центр экрана при первой инициализации
    MOUSE_X = (crate::framebuffer::width() as i32) / 2;
    MOUSE_Y = (crate::framebuffer::height() as i32) / 2;
}

/// Осушить буфер PS/2 от лишних байт.
unsafe fn drain() {
    for _ in 0..64 {
        let status = cpu::inb(PS2_CMD);
        if status & STATUS_OBF == 0 { break; }
        let _ = cpu::inb(PS2_DATA);
        core::hint::spin_loop();
    }
}

/// Опрос PS/2 мыши: читает готовые auxiliary-байты, собирает 3-байтный
/// пакет и при завершении обновляет глобальное состояние курсора.
pub unsafe fn poll() {
    if !MOUSE_ENABLED { return; }
    if !INIT_DONE {
        if INIT_FAILED {
            RETRY_TICKS += 1;
            if RETRY_TICKS >= RETRY_INTERVAL {
                RETRY_TICKS = 0;
                INIT_FAILED = false;
                init();
            }
            return;
        }
        init();
        if INIT_FAILED { return; }
    }

    let mut guard = 0;
    let mut desyncs = 0;
    while guard < 16 {
        guard += 1;
        let status = cpu::inb(PS2_CMD);
        if status & STATUS_OBF == 0 { break; }
        let byte = cpu::inb(PS2_DATA);

        // Читаем ТОЛЬКО auxiliary-байты (AUX-бит = 1), чтобы не воровать
        // клавиатурные байты у keyboard::poll(). Если AUX-бита нет —
        // оставляем байт для клавиатурного драйвера.
        if status & STATUS_AUX == 0 {
            continue;
        }

        PACKET_BUF[PACKET_POS] = byte;
        PACKET_POS += 1;

        if PACKET_POS == 3 {
            let b0 = PACKET_BUF[0];

            // Проверка синхронизации: бит 3 (0x08) должен быть 1 в первом байте
            if b0 & 0x08 == 0 {
                desyncs += 1;
                if desyncs >= 3 {
                    // Слишком много рассинхронов — сброс пакета
                    PACKET_POS = 0;
                    continue;
                }
                PACKET_BUF[0] = PACKET_BUF[1];
                PACKET_BUF[1] = PACKET_BUF[2];
                PACKET_POS = 2;
                continue;
            }

            // 9-bit signed: b0:bit4/5 = sign, data byte = unsigned magnitude
            let dx_raw = PACKET_BUF[1] as u32;
            let dy_raw = PACKET_BUF[2] as u32;
            let xs = (b0 & 0x10) != 0;
            let ys = (b0 & 0x20) != 0;

            let sens = crate::config::get_mouse_sensitivity() as i32;
            let dx = (if xs { (dx_raw as i32) - 256 } else { dx_raw as i32 }) * sens / 5;
            let dy = (if ys { (dy_raw as i32) - 256 } else { dy_raw as i32 }) * sens / 5;

            let buttons = b0 & 0x07;

            let fb_w = crate::framebuffer::width() as i32;
            let fb_h = crate::framebuffer::height() as i32;

            let new_x = (MOUSE_X + dx).max(0).min(fb_w - 1);
            let new_y = (MOUSE_Y - dy).max(0).min(fb_h - 1);

            if new_x != MOUSE_X || new_y != MOUSE_Y || buttons != MOUSE_BUTTONS {
                MOUSE_X = new_x;
                MOUSE_Y = new_y;
                MOUSE_BUTTONS = buttons;
                crate::usb::mouse_set_pos(new_x, new_y);
                crate::usb::mouse_set_buttons(buttons);
            }

            PACKET_POS = 0;
        }
    }
}

/// Принудительно переинициализировать PS/2 мышь (для команды mouse reset).
pub unsafe fn reset() {
    INIT_DONE = false;
    INIT_FAILED = false;
    PACKET_POS = 0;
    drain();
    init();
}

/// Проверить, инициализирована ли PS/2 мышь.
pub fn is_initialized() -> bool {
    unsafe { INIT_DONE }
}

/// Включить/выключить опрос PS/2 мыши.
pub unsafe fn set_enabled(on: bool) {
    MOUSE_ENABLED = on;
    if !on {
        PACKET_POS = 0;
    }
}

/// Проверить, включён ли опрос PS/2 мыши.
pub fn is_enabled() -> bool {
    unsafe { MOUSE_ENABLED }
}
