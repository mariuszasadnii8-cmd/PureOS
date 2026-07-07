//! Драйвер клавиатуры через прямой опрос PS/2-контроллёра (i8042).
//!
//! Порты 0x60 (данные) / 0x64 (статус+команды). Читаем scancode set 1
//! (контроллёр в режиме трансляции set2->set1, что оставляет UEFI/BIOS).
//!
//! Почему НЕ UEFI Simple Text Input: ядро в `kernel_main` ставит СВОИ GDT/IDT,
//! после чего вызов обратно в firmware (`ReadKeyStroke`) падает с #GP —
//! код прошивки грузит сегментный селектор из своей GDT, которой уже нет.
//! Прямой опрос портов вызовов firmware не делает и работает с cli (§5).

use crate::cpu;

/// Специальный код клавиши Meta (Windows/Command).
/// Не пересекается с ASCII-диапазоном и scancode-таблицами.
pub const KEY_META: u8 = 0xFB;

const PS2_DATA: u16 = 0x60;
const PS2_STATUS: u16 = 0x64;

const STATUS_OBF: u8 = 0x01; // output buffer full — есть байт для чтения
const STATUS_AUX: u8 = 0x20; // байт пришёл со второго порта (мышь) — игнорируем

const KEYBUF_SIZE: usize = 256;

static mut KEYBUF: [u8; KEYBUF_SIZE] = [0; KEYBUF_SIZE];
static mut KEYBUF_HEAD: u32 = 0;
static mut KEYBUF_TAIL: u32 = 0;
static mut INIT_DONE: bool = false;

// Состояние модификаторов/префиксов между скан-кодами.
static mut SHIFT: bool = false;
static mut EXTENDED: bool = false; // был префикс 0xE0 (стрелки и т.п.)

/// Инициализация: очистить буфер и осушить контроллёр от залежавшихся байт.
pub unsafe fn init() {
    KEYBUF_HEAD = 0;
    KEYBUF_TAIL = 0;
    SHIFT = false;
    EXTENDED = false;
    // Сбросить всё, что контроллёр накопил до старта ядра.
    let mut guard = 0;
    while guard < 32 && (cpu::inb(PS2_STATUS) & STATUS_OBF) != 0 {
        let _ = cpu::inb(PS2_DATA);
        guard += 1;
    }
    INIT_DONE = true;
}

/// Осушить аппаратный буфер контроллёра в кольцевой буфер клавиш.
/// Неблокирующий: читает все готовые байты и возвращается.
pub unsafe fn poll() {
    if !INIT_DONE { init(); }
    let mut guard = 0;
    while guard < 64 {
        let status = cpu::inb(PS2_STATUS);
        if status & STATUS_OBF == 0 { break; } // данных больше нет
        let code = cpu::inb(PS2_DATA);
        guard += 1;
        if status & STATUS_AUX != 0 { continue; } // байт от мыши — пропустить
        handle_scancode(code);
    }
}

/// Прочитать символ (неблокирующий). Сам подкачивает из железа, поэтому
/// работает и у вызовов, которые не зовут `poll()` (barrel, syscall input).
pub unsafe fn read_key() -> Option<u8> {
    poll();
    let tail = KEYBUF_TAIL;
    if tail == KEYBUF_HEAD {
        None
    } else {
        let ch = KEYBUF[tail as usize];
        KEYBUF_TAIL = (tail + 1) % KEYBUF_SIZE as u32;
        Some(ch)
    }
}

/// Обработать один scancode set 1: обновить модификаторы или положить ASCII.
unsafe fn handle_scancode(code: u8) {
    // Префикс расширенных клавиш — саму клавишу за ним пока игнорируем.
    if code == 0xE0 {
        EXTENDED = true;
        return;
    }
    // Meta/Windows key (0x5B left, 0x5C right) — пуск-меню
    if code == 0x5B || code == 0x5C {
        push_buf(KEY_META);
        return;
    }
    match code {
        0x2A | 0x36 => { SHIFT = true; return; }   // LShift/RShift нажаты
        0xAA | 0xB6 => { SHIFT = false; return; }  // LShift/RShift отпущены
        _ => {}
    }
    // Отпускание любой другой клавиши (бит 7) — не печатаем.
    if code & 0x80 != 0 {
        EXTENDED = false;
        return;
    }
    // Расширенная клавиша (стрелки/Ins/Del и т.п.) — пропускаем.
    if EXTENDED {
        EXTENDED = false;
        return;
    }
    let table = if SHIFT { &SHIFT_MAP } else { &NORMAL_MAP };
    let ch = table[(code & 0x7F) as usize];
    if ch != 0 {
        push_buf(ch);
    }
}

unsafe fn push_buf(ch: u8) {
    let head = KEYBUF_HEAD;
    let next = (head + 1) % KEYBUF_SIZE as u32;
    if next != KEYBUF_TAIL {
        KEYBUF[head as usize] = ch;
        KEYBUF_HEAD = next;
    }
}

// ===================================================================
// Таблицы трансляции scancode set 1 -> ASCII (индекс = make-код 0x00..0x7F).
// 0 = клавиша без печатного символа (модификаторы, F-клавиши, неизвестные).
// ===================================================================

static NORMAL_MAP: [u8; 128] = {
    let mut m = [0u8; 128];
    m[0x01] = 0x1B; // Esc
    m[0x02] = b'1'; m[0x03] = b'2'; m[0x04] = b'3'; m[0x05] = b'4'; m[0x06] = b'5';
    m[0x07] = b'6'; m[0x08] = b'7'; m[0x09] = b'8'; m[0x0A] = b'9'; m[0x0B] = b'0';
    m[0x0C] = b'-'; m[0x0D] = b'=';
    m[0x0E] = 0x08; // Backspace
    m[0x0F] = b'\t';
    m[0x10] = b'q'; m[0x11] = b'w'; m[0x12] = b'e'; m[0x13] = b'r'; m[0x14] = b't';
    m[0x15] = b'y'; m[0x16] = b'u'; m[0x17] = b'i'; m[0x18] = b'o'; m[0x19] = b'p';
    m[0x1A] = b'['; m[0x1B] = b']';
    m[0x1C] = b'\n'; // Enter
    m[0x1E] = b'a'; m[0x1F] = b's'; m[0x20] = b'd'; m[0x21] = b'f'; m[0x22] = b'g';
    m[0x23] = b'h'; m[0x24] = b'j'; m[0x25] = b'k'; m[0x26] = b'l'; m[0x27] = b';';
    m[0x28] = b'\''; m[0x29] = b'`';
    m[0x2B] = b'\\';
    m[0x2C] = b'z'; m[0x2D] = b'x'; m[0x2E] = b'c'; m[0x2F] = b'v'; m[0x30] = b'b';
    m[0x31] = b'n'; m[0x32] = b'm'; m[0x33] = b','; m[0x34] = b'.'; m[0x35] = b'/';
    m[0x37] = b'*'; // keypad *
    m[0x39] = b' ';
    m
};

static SHIFT_MAP: [u8; 128] = {
    let mut m = [0u8; 128];
    m[0x01] = 0x1B;
    m[0x02] = b'!'; m[0x03] = b'@'; m[0x04] = b'#'; m[0x05] = b'$'; m[0x06] = b'%';
    m[0x07] = b'^'; m[0x08] = b'&'; m[0x09] = b'*'; m[0x0A] = b'('; m[0x0B] = b')';
    m[0x0C] = b'_'; m[0x0D] = b'+';
    m[0x0E] = 0x08;
    m[0x0F] = b'\t';
    m[0x10] = b'Q'; m[0x11] = b'W'; m[0x12] = b'E'; m[0x13] = b'R'; m[0x14] = b'T';
    m[0x15] = b'Y'; m[0x16] = b'U'; m[0x17] = b'I'; m[0x18] = b'O'; m[0x19] = b'P';
    m[0x1A] = b'{'; m[0x1B] = b'}';
    m[0x1C] = b'\n';
    m[0x1E] = b'A'; m[0x1F] = b'S'; m[0x20] = b'D'; m[0x21] = b'F'; m[0x22] = b'G';
    m[0x23] = b'H'; m[0x24] = b'J'; m[0x25] = b'K'; m[0x26] = b'L'; m[0x27] = b':';
    m[0x28] = b'"'; m[0x29] = b'~';
    m[0x2B] = b'|';
    m[0x2C] = b'Z'; m[0x2D] = b'X'; m[0x2E] = b'C'; m[0x2F] = b'V'; m[0x30] = b'B';
    m[0x31] = b'N'; m[0x32] = b'M'; m[0x33] = b'<'; m[0x34] = b'>'; m[0x35] = b'?';
    m[0x37] = b'*';
    m[0x39] = b' ';
    m
};
