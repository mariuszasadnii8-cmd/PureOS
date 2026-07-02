//! Драйвер клавиатуры через UEFI Simple Text Input Protocol.
//!
//! Без PS/2, без портов 0x60/0x64, без сканкодов. Всё читается напрямую
//! из UEFI ConIn->ReadKeyStroke. Неблокирующий опрос, кольцевой буфер.
//!
//! UEFI остаётся доступен, т.к. ядро не вызывает ExitBootServices.

use crate::uefi;

const KEYBUF_SIZE: usize = 256;

static mut KEYBUF: [u8; KEYBUF_SIZE] = [0; KEYBUF_SIZE];
static mut KEYBUF_HEAD: u32 = 0;
static mut KEYBUF_TAIL: u32 = 0;
static mut INIT_DONE: bool = false;

/// Инициализация (обнуление буфера).
pub unsafe fn init() {
    KEYBUF_HEAD = 0;
    KEYBUF_TAIL = 0;
    INIT_DONE = true;
}

/// Опросить UEFI Simple Text Input и прочитать все доступные клавиши.
/// Вызывается из планировщика (shell::run).
pub unsafe fn poll() {
    if !INIT_DONE { init(); }
    loop {
        match uefi::read_key() {
            Some(ch) => push_buf(ch),
            None => break,
        }
    }
}

/// Прочитать символ из буфера (неблокирующий).
pub unsafe fn read_key() -> Option<u8> {
    let tail = KEYBUF_TAIL;
    if tail == KEYBUF_HEAD {
        None
    } else {
        let ch = KEYBUF[tail as usize];
        KEYBUF_TAIL = (tail + 1) % KEYBUF_SIZE as u32;
        Some(ch)
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
