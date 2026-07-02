//! UEFI-обёртки — доступ к SystemTable, ConOut, ConIn, RuntimeServices.
//!
//! Ядро НЕ вызывает ExitBootServices, поэтому UEFI-протоколы доступны весь рантайм.
//! Используем `uefi` crate для типов, raw FFI для zero-alloc вызовов.

// ---- Offsets in EFI System Table (x86_64) ----
const ST_OFF_CON_OUT: u64 = 64;
const ST_OFF_RUNTIME: u64 = 80;

// EFI_SIMPLE_TEXT_INPUT_PROTOCOL
const STIP_OFF_READ_KEY: u64 = 8;

// EFI_SIMPLE_TEXT_OUTPUT_PROTOCOL
const STOP_OFF_OUTPUT_STRING: u64 = 8;
const STOP_OFF_CLEAR_SCREEN: u64 = 48;

// EFI_RUNTIME_SERVICES: ResetSystem
const RT_OFF_RESET_SYSTEM: u64 = 88;

static mut UEFI_CON_OUT: u64 = 0;
static mut UEFI_CON_IN: u64 = 0;
static mut UEFI_RT: u64 = 0;
static mut UEFI_READY: bool = false;

/// Инициализировать UEFI-обёртки. Вызывается из kernel_main.
pub unsafe fn init(system_table: u64, con_in: u64) {
    let st = system_table as *const u8;
    UEFI_CON_OUT = *(st.add(ST_OFF_CON_OUT as usize) as *const u64);
    UEFI_CON_IN = con_in;
    UEFI_RT = *(st.add(ST_OFF_RUNTIME as usize) as *const u64);
    UEFI_READY = true;

    // Также регистрируем SystemTable в uefi crate (для доступа через system::*)
    let st_ptr = system_table as *const uefi_raw::table::system::SystemTable;
    uefi::table::set_system_table(st_ptr);
}

pub fn is_ready() -> bool { unsafe { UEFI_READY } }

// ===================================================================
// Simple Text Output (ConOut) — через raw FFI (zero-alloc)
// ===================================================================

/// Вывести ASCII-строку через UEFI ConOut.
pub fn con_out(msg: &[u8]) {
    if !is_ready() { return; }

    let con_out_ptr = unsafe { UEFI_CON_OUT };
    let out_fn = unsafe { *((con_out_ptr + STOP_OFF_OUTPUT_STRING) as *const u64) };
    if out_fn == 0 { return; }

    let mut u16buf = [0u16; 256];
    let len = msg.len().min(255);
    for i in 0..len { u16buf[i] = msg[i] as u16; }
    u16buf[len] = 0;

    unsafe {
        let f: extern "win64" fn(u64, *mut u16) -> usize = core::mem::transmute(out_fn);
        f(con_out_ptr, u16buf.as_mut_ptr());
    }
}

/// Очистить UEFI консоль.
pub fn clear_screen() {
    if !is_ready() { return; }

    let con_out_ptr = unsafe { UEFI_CON_OUT };
    let clear_fn = unsafe { *((con_out_ptr + STOP_OFF_CLEAR_SCREEN) as *const u64) };
    if clear_fn == 0 { return; }

    unsafe {
        let f: extern "win64" fn(u64) -> usize = core::mem::transmute(clear_fn);
        f(con_out_ptr);
    }
}

// ===================================================================
// Simple Text Input (ConIn)
// ===================================================================

#[repr(C)]
struct EfiInputKey { scan_code: u16, unicode_char: u16 }

/// Прочитать нажатую клавишу (неблокирующий).
pub fn read_key() -> Option<u8> {
    if !is_ready() { return None; }

    let con_in = unsafe { UEFI_CON_IN };
    let read_fn = unsafe { *((con_in + STIP_OFF_READ_KEY) as *const u64) };
    if read_fn == 0 { return None; }

    let mut key = EfiInputKey { scan_code: 0, unicode_char: 0 };
    let status: usize = unsafe {
        let f: extern "win64" fn(u64, *mut EfiInputKey) -> usize = core::mem::transmute(read_fn);
        f(con_in, &mut key)
    };

    if status != 0 { return None; }

    match key.unicode_char {
        0 => None,
        0x08 | 0x7F => Some(0x7F),
        0x0D | 0x0A => Some(b'\n'),
        0x09 => Some(b'\t'),
        0x1B | 0x20..=0x7E => Some(key.unicode_char as u8),
        _ => None,
    }
}

// ===================================================================
// Runtime Services — reboot, shutdown
// ===================================================================

fn do_reset(reset_type: u32) -> ! {
    if !is_ready() { loop { unsafe { core::arch::asm!("hlt") } } }
    let rt_ptr = unsafe { UEFI_RT };
    let reset_fn = unsafe { *((rt_ptr + RT_OFF_RESET_SYSTEM) as *const u64) };
    if reset_fn != 0 {
        unsafe {
            let f: extern "win64" fn(u32, usize, usize, *const u8) -> () =
                core::mem::transmute(reset_fn);
            f(reset_type, 0, 0, core::ptr::null());
        }
    }
    loop { unsafe { core::arch::asm!("hlt") } }
}

pub fn reset_system() -> ! { do_reset(0) }
pub fn shutdown() -> ! { do_reset(2) }
