//! USB-стек: EHCI (USB 2.0) + HID-клавиатура (boot protocol) + HID-мышь.
//! Zero-Alloc: статические массивы для QH/TD, device table.
//!
//! Порядок инициализации:
//!   1. usb::init() — найти EHCI на PCI, инициализировать HC
//!   2. usb::poll() — опрос root hub, перечисление устройств
//!   3. usb::keyboard_read() — прочитать клавишу с USB HID keyboard
//!   4. usb::mouse_poll() — обновить позицию курсора + отрисовать

mod ehci;
mod hid;

use core::ptr::write_volatile;

use crate::terminal;

// ---------------------------------------------------------------------------
// Константы USB
// ---------------------------------------------------------------------------

/// Класс USB-устройства.
pub const CLASS_HID: u8 = 3;

/// Стандартные USB request types (bmRequestType).
pub const REQ_TYPE_IN: u8 = 0xA1;  // Device-to-host, Class, Interface
pub const REQ_TYPE_OUT: u8 = 0x21; // Host-to-device, Class, Interface
pub const REQ_STD_DEV_IN: u8 = 0x80; // Device-to-host, Standard, Device

/// Стандартные USB request.
pub const REQ_GET_DESCRIPTOR: u8 = 6;
pub const REQ_SET_ADDRESS: u8 = 5;
pub const REQ_SET_CONFIGURATION: u8 = 9;
pub const REQ_SET_PROTOCOL: u8 = 11;
pub const REQ_SET_IDLE: u8 = 10;

/// Типы дескрипторов.
pub const DESC_DEVICE: u8 = 1;
pub const DESC_CONFIG: u8 = 2;
pub const DESC_INTERFACE: u8 = 4;
pub const DESC_ENDPOINT: u8 = 5;
pub const DESC_HID: u8 = 0x21;

/// Максимальное число устройств.
const MAX_DEVICES: usize = 8;

/// Размер буфера для прерываний.
const INT_BUF_SIZE: usize = 64;

// ---------------------------------------------------------------------------
// Структуры данных USB
// ---------------------------------------------------------------------------

/// Состояние устройства.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq)]
enum DevState {
    Detached = 0,
    Addressed = 1,
    Configured = 2,
    Ready = 3,
}

/// USB-устройство.
#[derive(Clone, Copy)]
struct UsbDevice {
    valid: bool,
    port: u8,              // порт root hub
    speed: u8,             // 0=full, 1=low, 2=high
    address: u8,           // назначенный адрес (1-127)
    state: DevState,
    vendor: u16,
    product: u16,
    device_class: u8,
    subclass: u8,
    protocol: u8,
    // Для HID keyboard
    int_in_endpoint: u8,   // адрес endpoint для interrupt IN
    int_interval: u8,      // интервал опроса (в ms/2^...)
    buf: [u8; INT_BUF_SIZE], // буфер для interrupt transfer
}

/// Таблица устройств.
static mut USB_DEVICES: [UsbDevice; MAX_DEVICES] = [UsbDevice {
    valid: false, port: 0, speed: 0, address: 0,
    state: DevState::Detached, vendor: 0, product: 0,
    device_class: 0, subclass: 0, protocol: 0,
    int_in_endpoint: 0, int_interval: 0, buf: [0; INT_BUF_SIZE],
}; MAX_DEVICES];

/// Следующий свободный адрес.
static mut NEXT_ADDR: u8 = 1;

/// Флаг готовности USB.
static mut USB_READY: bool = false;

// ---------------------------------------------------------------------------
// Публичный API
// ---------------------------------------------------------------------------

/// Инициализация USB-стека.
pub unsafe fn init() {
    terminal::write(b"[USB] Scanning PCI for EHCI controller...\n");

    match ehci::find_controller() {
        Some(io_base) => {
            terminal::write(b"[USB] EHCI at PCI, MMIO base: 0x");
            terminal::write_hex(io_base);
            terminal::write(b"\n");

            if ehci::init_controller(io_base) {
                terminal::write(b"[USB] EHCI initialized.\n");
                USB_READY = true;
            } else {
                terminal::write(b"[USB] EHCI init failed!\n");
            }
        }
        None => {
            terminal::write(b"[USB] No EHCI controller found.\n");
        }
    }
}

/// Периодический опрос USB (root hub + устройства).
pub unsafe fn poll() {
    if !USB_READY { return; }
    if !ehci::is_initialized() { return; }

    ehci::poll_root_hub();
    ehci::poll_interrupts();
}

/// Прочитать клавишу с USB HID keyboard (если есть).
pub fn key_read() -> Option<u8> {
    hid::read_key()
}

/// Обновить курсор мыши.
pub unsafe fn mouse_poll() {
    hid::update_cursor();
}

/// Инициализировать курсор мыши.
pub unsafe fn mouse_init() {
    hid::init_mouse();
}

/// Скрыть курсор (восстановить фон).
pub unsafe fn mouse_hide() {
    hid::hide_cursor();
}

/// Получить позицию курсора.
pub fn mouse_pos() -> (i32, i32) {
    hid::mouse_pos()
}

/// Получить кнопки мыши.
pub fn mouse_buttons() -> u8 {
    hid::mouse_buttons()
}

/// Проверить, готов ли USB.
pub fn is_ready() -> bool {
    unsafe { USB_READY }
}

/// Показать список USB-устройств.
pub unsafe fn cmd_usb(args: &[u8]) {
    if !USB_READY {
        terminal::write(b"USB not initialized.\n");
        return;
    }

    // Parse subcommand
    let trimmed = trim(args);
    if trimmed == b"test" {
        cmd_usb_test();
        return;
    }
    if trimmed == b"scan" {
        // Force re-enumeration
        ehci::poll_root_hub();
        terminal::write(b"Re-scan complete.\n");
        return;
    }

    // Default: show status
    terminal::write(b"USB Status:\n");
    terminal::write(b"  EHCI: ");
    if ehci::is_initialized() {
        terminal::write(b"ready\n");
    } else {
        terminal::write(b"not ready\n");
    }
    terminal::write(b"  Devices:\n");
    let mut found = false;
    for i in 0..MAX_DEVICES {
        let d = &USB_DEVICES[i];
        if d.valid {
            found = true;
            terminal::write(b"    [");
            terminal::write_num(i as u64);
            terminal::write(b"] ");
            class_name(d.device_class);
            terminal::write(b" Port ");
            terminal::write_num(d.port as u64);
            terminal::write(b" Addr ");
            terminal::write_num(d.address as u64);
            terminal::write(b" Speed ");
            terminal::write_num(d.speed as u64);
            terminal::write(b" 0x");
            write_hex_short(d.vendor as u64);
            terminal::write(b":0x");
            write_hex_short(d.product as u64);
            terminal::write(b"\n");
        }
    }
    if !found {
        terminal::write(b"    (none)\n");
    }
}

/// Вывести название класса USB.
fn class_name(cls: u8) {
    let name: &[u8] = match cls {
        1 => b"Audio",
        2 => b"Comm",
        3 => b"HID",
        5 => b"Physical",
        6 => b"Image",
        7 => b"Printer",
        8 => b"Storage",
        9 => b"Hub",
        10 => b"Data",
        11 => b"SmartCard",
        12 => b"ContentSec",
        13 => b"Video",
        14 => b"Health",
        0xDC => b"Diagnostic",
        0xE0 => b"Wireless",
        0xEF => b"Misc",
        0xFF => b"Vendor",
        _ => b"Unknown",
    };
    terminal::write(name);
}

/// Вывод 16-битного числа в hex (4 hex-цифры).
fn write_hex_short(val: u64) {
    let hex = b"0123456789abcdef";
    let mut buf = [0u8; 4];
    for i in 0..4 {
        buf[i] = hex[((val >> ((3 - i) * 4)) & 0xF) as usize];
    }
    terminal::write(&buf);
}

/// Interactive keyboard test: echo keypresses until ESC.
unsafe fn cmd_usb_test() {
    terminal::write(b"USB Keyboard Test: press keys (ESC to exit, '~' for escape codes)\n");
    loop {
        crate::usb::poll();
        if let Some(ch) = crate::usb::key_read() {
            if ch == 0x1B { // ESC
                terminal::write(b"\nTest ended.\n");
                return;
            }
            let hex = b"0123456789abcdef";
            let mut buf = [0u8; 4];
            buf[0] = b'\'';
            buf[1] = ch;
            buf[2] = b'\'';
            buf[3] = b' ';
            terminal::write(&buf);
            terminal::write(b"0x");
            buf[0] = hex[(ch >> 4) as usize];
            buf[1] = hex[(ch & 0xF) as usize];
            terminal::write(&buf[..2]);
            terminal::write(b"  ");
        }
        core::arch::asm!("pause", options(nomem, nostack, preserves_flags));
    }
}

/// Interactive mouse test.
pub unsafe fn cmd_mouse(_args: &[u8]) {
    terminal::write(b"Mouse state:\n");
    terminal::write(b"  Position: ");
    let (x, y) = hid::mouse_pos();
    terminal::write_num(x as u64);
    terminal::write(b", ");
    terminal::write_num(y as u64);
    terminal::write(b"\n  Buttons: ");
    let b = hid::mouse_buttons();
    if b & 1 != 0 { terminal::write(b"LEFT "); }
    if b & 2 != 0 { terminal::write(b"RIGHT "); }
    if b & 4 != 0 { terminal::write(b"MIDDLE "); }
    if b == 0 { terminal::write(b"none"); }
    terminal::write(b"\n  Cursor visible: ");
    terminal::write(b"yes");
    terminal::write(b"\n");
}

/// Trim leading/trailing whitespace.
fn trim(s: &[u8]) -> &[u8] {
    let mut start = 0;
    while start < s.len() && s[start] <= b' ' { start += 1; }
    let mut end = s.len();
    while end > start && s[end - 1] <= b' ' { end -= 1; }
    &s[start..end]
}

// ---------------------------------------------------------------------------
// Вспомогательные функции
// ---------------------------------------------------------------------------

/// Найти свободный слот в таблице устройств.
unsafe fn alloc_device() -> Option<usize> {
    for i in 0..MAX_DEVICES {
        if !USB_DEVICES[i].valid {
            return Some(i);
        }
    }
    None
}

/// Назначить адрес USB-устройству.
unsafe fn assign_address(dev_idx: usize, port: u8, speed: u8) -> Option<u8> {
    let addr = NEXT_ADDR;
    if addr == 0 { return None; } // overflow
    NEXT_ADDR = NEXT_ADDR.wrapping_add(1);

    USB_DEVICES[dev_idx] = UsbDevice {
        valid: true,
        port,
        speed,
        address: addr,
        state: DevState::Addressed,
        vendor: 0,
        product: 0,
        device_class: 0,
        subclass: 0,
        protocol: 0,
        int_in_endpoint: 0,
        int_interval: 0,
        buf: [0; INT_BUF_SIZE],
    };
    Some(addr)
}

/// Прочитать дескриптор устройства (8 байт, первые 8 для определения max packet).
pub unsafe fn desc_device_max_packet(addr: u8, speed: u8) -> Option<u8> {
    let mut buf = [0u8; 8];
    let setup = SetupPacket {
        bmRequestType: REQ_STD_DEV_IN,
        bRequest: REQ_GET_DESCRIPTOR,
        wValue: (DESC_DEVICE as u16) << 8,
        wIndex: 0,
        wLength: 8,
    };
    if ehci::control_transfer(addr, speed, &setup, None, Some(&mut buf)) {
        Some(buf[7]) // bMaxPacketSize0
    } else {
        None
    }
}

/// Прочитать полный дескриптор устройства.
unsafe fn desc_device(addr: u8, speed: u8, buf: &mut [u8; 18]) -> bool {
    let setup = SetupPacket {
        bmRequestType: REQ_STD_DEV_IN,
        bRequest: REQ_GET_DESCRIPTOR,
        wValue: (DESC_DEVICE as u16) << 8,
        wIndex: 0,
        wLength: 18,
    };
    ehci::control_transfer(addr, speed, &setup, None, Some(buf))
}

/// Установить адрес устройства.
unsafe fn set_address(_addr: u8, speed: u8, new_addr: u8) -> bool {
    let setup = SetupPacket {
        bmRequestType: 0x00, // Host-to-device, Standard, Device
        bRequest: REQ_SET_ADDRESS,
        wValue: new_addr as u16,
        wIndex: 0,
        wLength: 0,
    };
    // SET_ADDRESS использует адрес 0 (default address)
    ehci::control_transfer_with_addr(0, speed, &setup, None, None)
}

/// Установить конфигурацию устройства.
unsafe fn set_configuration(addr: u8, speed: u8, config: u8) -> bool {
    let setup = SetupPacket {
        bmRequestType: 0x00,
        bRequest: REQ_SET_CONFIGURATION,
        wValue: config as u16,
        wIndex: 0,
        wLength: 0,
    };
    ehci::control_transfer(addr, speed, &setup, None, None)
}

// ---------------------------------------------------------------------------
// Setup packet (8 байт)
// ---------------------------------------------------------------------------

#[repr(C, packed)]
pub struct SetupPacket {
    pub bmRequestType: u8,
    pub bRequest: u8,
    pub wValue: u16,
    pub wIndex: u16,
    pub wLength: u16,
}

/// Записать 8-байтный setup packet в буфер (little-endian USB).
pub unsafe fn write_setup_buf(buf: *mut u8, sp: &SetupPacket) {
    write_volatile(buf, sp.bmRequestType);
    write_volatile(buf.add(1), sp.bRequest);
    write_volatile(buf.add(2) as *mut u16, sp.wValue);
    write_volatile(buf.add(4) as *mut u16, sp.wIndex);
    write_volatile(buf.add(6) as *mut u16, sp.wLength);
}


