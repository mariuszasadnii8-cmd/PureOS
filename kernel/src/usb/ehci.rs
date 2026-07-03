//! EHCI (USB 2.0) Host Controller driver.
//!
//! Регистры MMIO (BAR0), порты, async/periodic schedules.
//! Zero-Alloc: статические QH/qTD для всех транзакций.

use core::ptr::{addr_of, read_volatile, write_volatile};

use crate::cpu;
use crate::terminal;

use super::*;

// ---------------------------------------------------------------------------
// EHCI PCI
// ---------------------------------------------------------------------------

const PCI_CLASS: u8 = 0x0C;
const PCI_SUBCLASS: u8 = 0x03;
const PCI_PROGIF_EHCI: u8 = 0x20;

/// Найти EHCI контроллер на PCI. Возвращает MMIO base (BAR0).
pub unsafe fn find_controller() -> Option<u64> {
    for bus in 0..=0xFF {
        for slot in 0..32 {
            let vendor = cpu::pci_read32(bus, slot, 0, 0) & 0xFFFF;
            if vendor == 0 || vendor == 0xFFFF { continue; }
            let class = cpu::pci_read32(bus, slot, 0, 8) >> 24;
            let subclass = (cpu::pci_read32(bus, slot, 0, 8) >> 16) & 0xFF;
            let prog_if = (cpu::pci_read32(bus, slot, 0, 8) >> 8) & 0xFF;
            if class == PCI_CLASS as u32 && subclass == PCI_SUBCLASS as u32 && prog_if == PCI_PROGIF_EHCI as u32 {
                let bar0 = cpu::pci_read32(bus, slot, 0, 0x10);
                let mmio = (bar0 & 0xFFFFFFF0) as u64;
                if mmio != 0 { return Some(mmio); }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// EHCI MMIO регистры (для доступа через volatile)
// ---------------------------------------------------------------------------

struct EhciRegs {
    base: u64,
    caplength: u8,
    hcsparams: u32,
    hccparams: u32,
    op_base: u64,
}

static mut EHCI: Option<EhciRegs> = None;

#[inline(always)]
unsafe fn mmio_read(addr: u64, off: u64) -> u32 {
    read_volatile((addr + off) as *const u32)
}
#[inline(always)]
unsafe fn mmio_write(addr: u64, off: u64, val: u32) {
    write_volatile((addr + off) as *mut u32, val);
}

// Смещения operational registers (add op_base)
const CMD: u64 = 0x00;     // USBCMD
const STS: u64 = 0x04;     // USBSTS
const INTR: u64 = 0x08;    // USBINTR
const FRINDEX: u64 = 0x0C;
const CTRLDSSEGMENT: u64 = 0x10;
const PERIODICLISTBASE: u64 = 0x14;
const ASYNCLISTADDR: u64 = 0x18;
const CONFIGFLAG: u64 = 0x40;
const PORTSC: u64 = 0x44;  // + 4 * port

// Биты USBCMD
const CMD_RUN: u32 = 1;
const CMD_HCRESET: u32 = 2;
const CMD_ASYNC_ENABLE: u32 = 1 << 5;
const CMD_PERIODIC_ENABLE: u32 = 1 << 4;

// Биты USBSTS
const STS_HCHALTED: u32 = 1 << 0;
const STS_PCD: u32 = 1 << 2;    // Port change detect

// Биты PORTSC
const PORT_CONNECT: u32 = 1;
const PORT_ENABLE: u32 = 1 << 2;
const PORT_RESET: u32 = 1 << 8;
const PORT_LINESTATE: u32 = 0x3 << 10;
const PORT_SPEED: u32 = 0x3 << 26; // bits 27:26
const PORT_SPEED_SHIFT: u32 = 26;

/// Скорость по PORTSC.
unsafe fn port_speed(portsc: u32) -> u8 {
    // 0 = full (12 Mbps), 1 = low (1.5 Mbps), 2 = high (480 Mbps)
    ((portsc >> 26) & 0x3) as u8
}

// ---------------------------------------------------------------------------
// Структуры данных EHCI (QH + qTD)
// ---------------------------------------------------------------------------

// Queue Head: 48 байт, alignment 32
// Для простоты используем [u8; 48] и доступ по смещениям.
const QH_SIZE: usize = 48;
const QTD_SIZE: usize = 32;

// Смещения в QH:
const QH_NEXT: usize = 0;      // dword 0: next QH pointer
const QH_NEXT_ALT: usize = 4;  // dword 1: alternate next
const QH_TOKEN: usize = 8;     // dword 2: token (overlay)
const QH_BUF0: usize = 12;     // dword 3: buffer page 0
const QH_BUF1: usize = 16;     // dword 4: buffer page 1
const QH_BUF2: usize = 20;     // dword 5: buffer page 2
const QH_BUF3: usize = 24;     // dword 6: buffer page 3
const QH_BUF4: usize = 28;     // dword 7: buffer page 4
const QH_CURRENT: usize = 32;  // dword 8: current qTD pointer
const QH_SCRATCH1: usize = 36; // dword 9
const QH_SCRATCH2: usize = 40; // dword 10
const QH_SCRATCH3: usize = 44; // dword 11

// Смещения в qTD:
const QTD_NEXT: usize = 0;
const QTD_ALT: usize = 4;
const QTD_TOKEN: usize = 8;
const QTD_BUF0: usize = 12;
const QTD_BUF1: usize = 16;
const QTD_BUF2: usize = 20;
const QTD_BUF3: usize = 24;
const QTD_BUF4: usize = 28;

// Биты QH/QTD token
const TOKEN_ACTIVE: u32 = 1 << 7;
const TOKEN_ERROR: u32 = 0x7 << 8;  // bits 10:8
const TOKEN_PID_OUT: u32 = 0 << 0;
const TOKEN_PID_IN: u32 = 1 << 0;
const TOKEN_PID_SETUP: u32 = 2 << 0;
const TOKEN_DT_BIT0: u32 = 1 << 14;
const TOKEN_CERR: u32 = 3 << 10;
const TOKEN_C_PAGE: u32 = 0 << 12;
const TOKEN_NBYTES: u32 = 0x7FFF << 16;
const TOKEN_TOGGLE: u32 = 1 << 14;
const TOKEN_IOC: u32 = 1 << 15;

// --- Статический пул QH и qTD ---
const MAX_QH: usize = 8;
const MAX_QTD: usize = 32;

// Выравнивание: QH нужен 32-байт, qTD нужен 32-байт.
// Используем буфер с доп. padding для выравнивания.
#[repr(align(32))]
struct AlignedQh([u8; MAX_QH * QH_SIZE]);
#[repr(align(32))]
struct AlignedQtd([u8; MAX_QTD * QTD_SIZE]);

static mut QH_POOL: AlignedQh = AlignedQh([0; MAX_QH * QH_SIZE]);
static mut QTD_POOL: AlignedQtd = AlignedQtd([0; MAX_QTD * QTD_SIZE]);

static mut QH_ALLOC: usize = 0;
static mut QTD_ALLOC: usize = 0;

/// Выделить QH из пула.
unsafe fn qh_alloc() -> Option<*mut u8> {
    if QH_ALLOC >= MAX_QH { return None; }
    let idx = QH_ALLOC;
    QH_ALLOC += 1;
    Some(addr_of!(QH_POOL.0[idx * QH_SIZE]) as *mut u8)
}

/// Выделить qTD из пула.
unsafe fn qtd_alloc() -> Option<*mut u8> {
    if QTD_ALLOC >= MAX_QTD { return None; }
    let idx = QTD_ALLOC;
    QTD_ALLOC += 1;
    Some(addr_of!(QTD_POOL.0[idx * QTD_SIZE]) as *mut u8)
}

/// Сбросить аллокатор (при реинит).
unsafe fn qh_reset_alloc() { QH_ALLOC = 0; QTD_ALLOC = 0; }

// --- Фрейм-лист (1024 эл. × 4 байта = 4 КБ) ---
const FRAME_LIST_ENTRIES: usize = 1024;
type FrameList = [u32; FRAME_LIST_ENTRIES];
static mut FRAME_LIST: FrameList = [0; FRAME_LIST_ENTRIES];

// --- Async list head (QH) ---
static mut ASYNC_HEAD_QH: [u8; QH_SIZE] = [0; QH_SIZE];

// --- Async QH для контрольных передач ---
static mut CTRL_QH: [u8; QH_SIZE] = [0; QH_SIZE];
static mut CTRL_QTD_SETUP: [u8; QTD_SIZE] = [0; QTD_SIZE];
static mut CTRL_QTD_DATA: [u8; QTD_SIZE] = [0; QTD_SIZE];
static mut CTRL_QTD_STATUS: [u8; QTD_SIZE] = [0; QTD_SIZE];

// --- Periodic QH для interrupt (HID keyboard) ---
static mut INT_QH: [u8; QH_SIZE] = [0; QH_SIZE];
static mut INT_QTD: [u8; QTD_SIZE] = [0; QTD_SIZE];

// --- Periodic QH для interrupt (HID mouse) ---
static mut INT_QH_MOUSE: [u8; QH_SIZE] = [0; QH_SIZE];
static mut INT_QTD_MOUSE: [u8; QTD_SIZE] = [0; QTD_SIZE];

// Флаг инициализации.
static mut EHCI_READY: bool = false;

// ---------------------------------------------------------------------------
// Инициализация контроллера
// ---------------------------------------------------------------------------

pub unsafe fn init_controller(mmio_base: u64) -> bool {
    // Проверить, что MMIO адрес выглядит разумно.
    if mmio_base == 0 || mmio_base > 0xFFFF_FFFF { return false; }

    let caplength = read_volatile(mmio_base as *const u8);
    let hcsparams = read_volatile((mmio_base + 4) as *const u32);
    let hccparams = read_volatile((mmio_base + 8) as *const u32);
    let op_base = mmio_base + caplength as u64;
    let n_ports = ((hcsparams >> 24) & 0xF) as u8;

    terminal::write(b"[EHCI] CAPLENGTH=");
    terminal::write_num(caplength as u64);
    terminal::write(b" ports=");
    terminal::write_num(n_ports as u64);
    terminal::write(b"\n");

    EHCI = Some(EhciRegs {
        base: mmio_base, caplength, hcsparams, hccparams, op_base,
    });

    // Reset controller
    mmio_write(op_base, CMD, CMD_HCRESET);
    for _ in 0..1000000 {
        if mmio_read(op_base, CMD) & CMD_HCRESET == 0 { break; }
        core::hint::spin_loop();
    }

    // Wait for halt
    for _ in 0..100000 {
        if mmio_read(op_base, STS) & STS_HCHALTED != 0 { break; }
        core::hint::spin_loop();
    }

    // Set CTRLDSSEGMENT = 0 (32-bit DMA)
    mmio_write(op_base, CTRLDSSEGMENT, 0);

    // Set PERIODICLISTBASE
    let frame_list_phys = addr_of!(FRAME_LIST) as u32;
    mmio_write(op_base, PERIODICLISTBASE, frame_list_phys);

    // Set ASYNCLISTADDR
    let async_head_phys = addr_of!(ASYNC_HEAD_QH) as u32;
    mmio_write(op_base, ASYNCLISTADDR, async_head_phys);

    // Set CONFIGFLAG = 1 (route all ports to EHCI)
    mmio_write(op_base, CONFIGFLAG, 1);

    // Run: CMD.RUN = 1, ASYNC_ENABLE = 1, PERIODIC_ENABLE = 1
    mmio_write(op_base, CMD, CMD_RUN | CMD_ASYNC_ENABLE | CMD_PERIODIC_ENABLE);

    // Wait for run (HCHalted clears)
    for _ in 0..100000 {
        if mmio_read(op_base, STS) & STS_HCHALTED == 0 { break; }
        core::hint::spin_loop();
    }

    if mmio_read(op_base, STS) & STS_HCHALTED != 0 {
        terminal::write(b"[EHCI] Failed to start!\n");
        return false;
    }

    // Init async head QH
    let f = |q: *mut u8| write_volatile(q as *mut u32, 0u32);
    for i in 0..QH_SIZE/4 {
        let p = (addr_of!(ASYNC_HEAD_QH) as *mut u8).wrapping_add(i * 4);
        f(p);
    }
    // Set H flag (bit 0 = 1) → QH is the async head
    write_volatile(addr_of!(ASYNC_HEAD_QH) as *mut u32, 1);

    // Init control QH
    for i in 0..QH_SIZE/4 {
        let p = (addr_of!(CTRL_QH) as *mut u8).wrapping_add(i * 4);
        f(p);
    }
    // Link CTRL_QH after head (bit 0 = 1 for QH type)
    write_volatile(addr_of!(ASYNC_HEAD_QH) as *mut u32,
                   (addr_of!(CTRL_QH) as u32) | 2); // bit 1 = 1 (QH), bit 0 = 0 in next

    // Init periodic frame list (all pointing to INT_QH or 0)
    for i in 0..FRAME_LIST_ENTRIES {
        FRAME_LIST[i] = 0;
    }

    // Init interrupt QH (keyboard)
    for i in 0..QH_SIZE/4 {
        let p = (addr_of!(INT_QH) as *mut u8).wrapping_add(i * 4);
        f(p);
    }
    // Init interrupt QH (mouse)
    for i in 0..QH_SIZE/4 {
        let p = (addr_of!(INT_QH_MOUSE) as *mut u8).wrapping_add(i * 4);
        f(p);
    }

    // Link keyboard QH → mouse QH → terminate
    let mouse_qh_phys = addr_of!(INT_QH_MOUSE) as u32;
    write_volatile(addr_of!(INT_QH) as *mut u32, mouse_qh_phys | 2); // bit1 = 1 (QH), bit0 = 0
    write_volatile(addr_of!(INT_QH_MOUSE) as *mut u32, 1); // terminate

    EHCI_READY = true;
    true
}

pub fn is_initialized() -> bool {
    unsafe { EHCI_READY }
}

// ---------------------------------------------------------------------------
// Порты root hub
// ---------------------------------------------------------------------------

/// Опрос портов root hub.
pub unsafe fn poll_root_hub() {
    let Some(ref ehci) = EHCI else { return };
    let op = ehci.op_base;
    let n_ports = ((ehci.hcsparams >> 24) & 0xF) as u8;

    for port in 0..n_ports {
        let portsc = mmio_read(op, PORTSC + (port as u64) * 4);
        let connected = portsc & PORT_CONNECT != 0;
        let enabled = portsc & PORT_ENABLE != 0;

        // Check if already handled
        let already = {
            let mut found = false;
            for d in &USB_DEVICES {
                if d.valid && d.port == port { found = true; break; }
            }
            found
        };

        if connected && !enabled {
            // Port connect but not enabled — reset it
            terminal::write(b"[EHCI] Port ");
            terminal::write_num(port as u64);
            terminal::write(b" connect, resetting...\n");

            // Reset
            mmio_write(op, PORTSC + (port as u64) * 4, PORT_RESET);
            for _ in 0..100000 { core::hint::spin_loop(); }
            mmio_write(op, PORTSC + (port as u64) * 4, 0);
            for _ in 0..100000 { core::hint::spin_loop(); }

            let portsc2 = mmio_read(op, PORTSC + (port as u64) * 4);
            if portsc2 & PORT_ENABLE != 0 {
                let speed = port_speed(portsc2);
                terminal::write(b"[EHCI] Port ");
                terminal::write_num(port as u64);
                terminal::write(b" enabled, speed=");
                terminal::write_num(speed as u64);
                terminal::write(b"\n");
            }
        }

        if connected && enabled && !already {
            terminal::write(b"[EHCI] Enumerating device on port ");
            terminal::write_num(port as u64);
            terminal::write(b"...\n");
            enumerate_device(port);
        }

        if !connected && already {
            // Device removed
            for i in 0..MAX_DEVICES {
                if USB_DEVICES[i].valid && USB_DEVICES[i].port == port {
                    USB_DEVICES[i].valid = false;
                    terminal::write(b"[EHCI] Device removed on port ");
                    terminal::write_num(port as u64);
                    terminal::write(b"\n");
                }
            }
        }
    }
}

/// Перечислить устройство на порту.
unsafe fn enumerate_device(port: u8) {
    let Some(ref ehci) = EHCI else { return };
    let op = ehci.op_base;
    let portsc = mmio_read(op, PORTSC + (port as u64) * 4);
    let speed = port_speed(portsc);

    // Check if device is already in table
    for d in &USB_DEVICES {
        if d.valid && d.port == port { return; }
    }

    // Allocate device slot
    let dev_idx = match alloc_device() {
        Some(i) => i,
        None => {
            terminal::write(b"[USB] Device table full!\n");
            return;
        }
    };

    // Default address 0, get max packet size
    let _max_pkt = match desc_device_max_packet(0, speed) {
        Some(s) => s,
        None => {
            terminal::write(b"[USB] Failed to get device descriptor.\n");
            return;
        }
    };

    // Assign address
    let addr = match assign_address(dev_idx, port, speed) {
        Some(a) => a,
        None => {
            terminal::write(b"[USB] Address allocation failed.\n");
            return;
        }
    };

    // Send SET_ADDRESS
    if !set_address(0, speed, addr) {
        terminal::write(b"[USB] SET_ADDRESS failed.\n");
        USB_DEVICES[dev_idx].valid = false;
        return;
    }

    // Address now changes to new address
    let d = &mut USB_DEVICES[dev_idx];
    d.address = addr;
    d.state = DevState::Addressed;

    // Get device descriptor (full 18 bytes)
    let mut dev_desc = [0u8; 18];
    if !desc_device(addr, speed, &mut dev_desc) {
        terminal::write(b"[USB] Failed to read full device descriptor.\n");
        return;
    }

    d.vendor = u16::from(dev_desc[10]) | (u16::from(dev_desc[11]) << 8);
    d.product = u16::from(dev_desc[12]) | (u16::from(dev_desc[13]) << 8);
    d.device_class = dev_desc[4];
    d.subclass = dev_desc[5];
    d.protocol = dev_desc[6];

    // Get config descriptor (starts at offset 9 in dev desc)
    let config = dev_desc[17]; // bNumConfigurations, but we use first config
    if config == 0 {
        terminal::write(b"[USB] No configuration.\n");
        return;
    }

    // Set configuration 1
    if !set_configuration(addr, speed, 1) {
        terminal::write(b"[USB] SET_CONFIGURATION failed.\n");
        return;
    }
    d.state = DevState::Configured;

    // Check if HID
    if d.device_class == CLASS_HID {
        terminal::write(b"[USB] HID device found (keyboard)\n");
        setup_hid_device(dev_idx);
    }

    terminal::write(b"[USB] Device ");
    terminal::write_num(addr as u64);
    terminal::write(b": vendor=0x");
    terminal::write_hex(d.vendor as u64);
    terminal::write(b" product=0x");
    terminal::write_hex(d.product as u64);
    terminal::write(b" class=0x");
    terminal::write_hex(d.device_class as u64);
    terminal::write(b"\n");
}

// ---------------------------------------------------------------------------
// HID setup
// ---------------------------------------------------------------------------

/// Настроить HID-устройство (клавиатура в boot protocol).
unsafe fn setup_hid_device(dev_idx: usize) {
    let d = &USB_DEVICES[dev_idx];
    let addr = d.address;
    let speed = d.speed;

    // Get config descriptor (full) to find interrupt endpoint
    let mut config_buf = [0u8; 64];
    let setup = SetupPacket {
        bmRequestType: REQ_STD_DEV_IN,
        bRequest: REQ_GET_DESCRIPTOR,
        wValue: (DESC_CONFIG as u16) << 8,
        wIndex: 0,
        wLength: 64,
    };
    if !ehci::control_transfer(addr, speed, &setup, None, Some(&mut config_buf)) {
        terminal::write(b"[HID] Failed to get config descriptor\n");
        return;
    }
    if config_buf[0] < 9 { return; } // short descriptor

    // Parse interfaces/endpoints
    let total_len = u16::from(config_buf[2]) | (u16::from(config_buf[3]) << 8);
    let total_len = (total_len as usize).min(config_buf.len());

    // Find HID interface with interrupt IN endpoint
    let mut i = 9; // skip config descriptor (9 bytes)
    let mut hid_iface_found = false;
    while i < total_len {
        let len = config_buf[i] as usize;
        let desc_type = config_buf[i + 1];
        if len == 0 { break; }

        if desc_type == DESC_INTERFACE && i + 9 <= total_len {
            let iface_class = config_buf[i + 5];
            let iface_subclass = config_buf[i + 6];
            let _iface_protocol = config_buf[i + 7];

            if iface_class == CLASS_HID {
                hid_iface_found = true;

                // Set boot protocol
                let boot_setup = SetupPacket {
                    bmRequestType: REQ_TYPE_OUT,
                    bRequest: REQ_SET_PROTOCOL,
                    wValue: 0, // boot protocol
                    wIndex: config_buf[i + 2] as u16, // interface number
                    wLength: 0,
                };
                ehci::control_transfer(addr, speed, &boot_setup, None, None);

                // Set idle rate = 0 (no rate limiting)
                let idle_setup = SetupPacket {
                    bmRequestType: REQ_TYPE_OUT,
                    bRequest: REQ_SET_IDLE,
                    wValue: 0,
                    wIndex: config_buf[i + 2] as u16,
                    wLength: 0,
                };
                ehci::control_transfer(addr, speed, &idle_setup, None, None);

                // Store subclass for later endpoint routing
                let d = &mut USB_DEVICES[dev_idx];
                d.subclass = iface_subclass;
                d.protocol = _iface_protocol;
            }
        }

        if desc_type == DESC_ENDPOINT && hid_iface_found && i + 7 <= total_len {
            let ep_addr = config_buf[i + 2];
            let ep_attr = config_buf[i + 3];
            let ep_type = ep_attr & 0x03;

            // Interrupt IN endpoint
            if ep_type == 3 && (ep_addr & 0x80) != 0 {
                let interval = config_buf[i + 6];
                let d = &mut USB_DEVICES[dev_idx];
                d.int_in_endpoint = ep_addr;
                d.int_interval = interval;
                d.state = DevState::Ready;

                if d.subclass == 2 {
                    terminal::write(b"[HID] Mouse endpoint 0x");
                    terminal::write_hex(ep_addr as u64);
                    terminal::write(b" interval=");
                    terminal::write_num(interval as u64);
                    terminal::write(b"\n");
                    setup_int_qh_mouse(addr, speed, ep_addr);
                } else {
                    terminal::write(b"[HID] Keyboard endpoint 0x");
                    terminal::write_hex(ep_addr as u64);
                    terminal::write(b" interval=");
                    terminal::write_num(interval as u64);
                    terminal::write(b"\n");
                    setup_int_qh(addr, speed, ep_addr);
                }
                return;
            }
        }

        i = if i + len <= total_len { i + len } else { total_len };
    }
}

/// Настроить периодический QH для interrupt transfers.
unsafe fn setup_int_qh(addr: u8, _speed: u8, ep_addr: u8) {
    let qh = addr_of!(INT_QH) as *mut u32;

    // Clear QH
    for i in 0..QH_SIZE/4 {
        write_volatile(qh.add(i), 0u32);
    }

    // QH overlay:
    // dword 0: next = terminate (1)
    write_volatile(qh.add(0), 1);
    // dword 1: alt next = terminate (1)
    write_volatile(qh.add(1), 1);

    // Characteristic fields:
    // bit 27:16 = device address
    // bit 14:12 = endpoint number
    // bit 7:0 = max packet length (8 for keyboard boot report)
    let endp = (ep_addr & 0x0F) as u32;
    let dev_addr = addr as u32;
    let max_pkt = 8u32;
    let qh_char = (dev_addr << 16) | (endp << 12) | max_pkt;
    write_volatile(qh.add(2), qh_char);

    // qTD for the interrupt transfer
    let qtd = addr_of!(INT_QTD) as *mut u32;
    for i in 0..QTD_SIZE/4 {
        write_volatile(qtd.add(i), 0u32);
    }

    // Next = terminate
    write_volatile(qtd.add(0), 1u32);
    write_volatile(qtd.add(1), 1u32);

    // Token: active, IN, 8 bytes, toggle=0, IOC
    write_volatile(qtd.add(2), TOKEN_ACTIVE | TOKEN_PID_IN | (8 << 16) | TOKEN_IOC);

    // Buffer pointer to the device's buf
    let d = &mut USB_DEVICES[0]; // HID device must be at index 0 for simplicity
    let buf_phys = addr_of!(d.buf) as u32;
    write_volatile(qtd.add(3), buf_phys);
    // Buffers 1-4: 0 (8 bytes fits in one page)
    write_volatile(qtd.add(4), 0u32);
    write_volatile(qtd.add(5), 0u32);
    write_volatile(qtd.add(6), 0u32);
    write_volatile(qtd.add(7), 0u32);

    // Link qTD to QH
    let qtd_phys = addr_of!(INT_QTD) as u32;
    write_volatile(qh.add(3), qtd_phys); // overlay next qTD
    write_volatile(qh.add(4), 0u32);     // alt next
    write_volatile(qh.add(5), TOKEN_ACTIVE | TOKEN_PID_IN | (8 << 16) | TOKEN_IOC); // token
    write_volatile(qh.add(6), buf_phys); // buffer 0

    // Set frame list to point to INT_QH
    let qh_phys = addr_of!(INT_QH) as u32;
    for i in 0..FRAME_LIST_ENTRIES {
        // Set bit 1 to mark as QH (bit 0=0 for QH in periodic list)
        FRAME_LIST[i] = qh_phys | 2;
    }

    // Flush schedule by writing ASYNCLISTADDR (doorbell)
    // Actually for periodic, just command reg
}

/// Настроить периодический QH для mouse interrupt transfers.
unsafe fn setup_int_qh_mouse(addr: u8, _speed: u8, ep_addr: u8) {
    let qh = addr_of!(INT_QH_MOUSE) as *mut u32;

    // Clear QH (keep horizontal next = terminate)
    let next_val = read_volatile(qh);
    for i in 1..QH_SIZE/4 {
        write_volatile(qh.add(i), 0u32);
    }
    write_volatile(qh, next_val); // restore next pointer

    // QH characterisation:
    let endp = (ep_addr & 0x0F) as u32;
    let dev_addr = addr as u32;
    let max_pkt = 8u32; // boot mouse report is 3 bytes, round up to 8
    let qh_char = (dev_addr << 16) | (endp << 12) | max_pkt;
    write_volatile(qh.add(2), qh_char);

    // qTD for mouse
    let qtd = addr_of!(INT_QTD_MOUSE) as *mut u32;
    for i in 0..QTD_SIZE/4 {
        write_volatile(qtd.add(i), 0u32);
    }
    write_volatile(qtd.add(0), 1u32); // next = terminate
    write_volatile(qtd.add(1), 1u32); // alt = terminate

    // Find the mouse device buffer
    let mut buf_phys = 0u32;
    for d in &super::USB_DEVICES {
        if d.valid && d.state == DevState::Ready && d.device_class == CLASS_HID && d.subclass == 2 {
            buf_phys = addr_of!(d.buf) as u32;
            break;
        }
    }

    // Token: active, IN, 8 bytes, toggle=0, IOC
    write_volatile(qtd.add(2), TOKEN_ACTIVE | TOKEN_PID_IN | (8 << 16) | TOKEN_IOC);
    write_volatile(qtd.add(3), buf_phys);
    write_volatile(qtd.add(4), 0u32);
    write_volatile(qtd.add(5), 0u32);
    write_volatile(qtd.add(6), 0u32);
    write_volatile(qtd.add(7), 0u32);

    // Link qTD to QH overlay
    let qtd_phys = addr_of!(INT_QTD_MOUSE) as u32;
    write_volatile(qh.add(3), qtd_phys);
    write_volatile(qh.add(4), 0u32);
    write_volatile(qh.add(5), TOKEN_ACTIVE | TOKEN_PID_IN | (8 << 16) | TOKEN_IOC);
    write_volatile(qh.add(6), buf_phys);

    // No need to update frame list — keyboard QH already points here via next
}

// ---------------------------------------------------------------------------
// Control transfer (async schedule)
// ---------------------------------------------------------------------------

/// Выполнить контрольную передачу через async schedule.
pub unsafe fn control_transfer(addr: u8, _speed: u8, setup: &SetupPacket,
                                data_out: Option<&[u8]>, data_in: Option<&mut [u8]>) -> bool {
    control_transfer_with_addr(addr, _speed, setup, data_out, data_in)
}

pub unsafe fn control_transfer_with_addr(target_addr: u8, _speed: u8,
                                          setup: &SetupPacket,
                                          data_out: Option<&[u8]>,
                                          data_in: Option<&mut [u8]>) -> bool {
    let qh = addr_of!(CTRL_QH) as *mut u32;
    let qtd_setup = addr_of!(CTRL_QTD_SETUP) as *mut u32;
    let qtd_data = addr_of!(CTRL_QTD_DATA) as *mut u32;
    let qtd_status = addr_of!(CTRL_QTD_STATUS) as *mut u32;

    // Clear all
    for i in 0..QH_SIZE/4 { write_volatile(qh.add(i), 0u32); }
    for i in 0..QTD_SIZE/4 { write_volatile(qtd_setup.add(i), 0u32); }
    for i in 0..QTD_SIZE/4 { write_volatile(qtd_data.add(i), 0u32); }
    for i in 0..QTD_SIZE/4 { write_volatile(qtd_status.add(i), 0u32); }

    let dev_addr = target_addr as u32;
    let max_pkt = 64u32; // assume high-speed
    let endp = 0u32;
    let qh_char = (dev_addr << 16) | (endp << 12) | max_pkt;
    write_volatile(qh.add(2), qh_char);

    // Write setup packet
    write_setup_buf(addr_of!(CTRL_QTD_SETUP) as *mut u8, setup);

    // qTD for setup stage
    write_volatile(qtd_setup.add(0), 1u32); // next = terminate
    write_volatile(qtd_setup.add(1), 1u32); // alt = terminate
    let setup_len = 8u32;
    let setup_token = TOKEN_ACTIVE | TOKEN_PID_SETUP | (setup_len << 16) | TOKEN_IOC;
    write_volatile(qtd_setup.add(2), setup_token);
    let setup_buf_phys = addr_of!(CTRL_QTD_SETUP) as u32;
    write_volatile(qtd_setup.add(3), setup_buf_phys);
    // data stage qTD
    let data_phys = addr_of!(CTRL_QTD_DATA) as u32;
    write_volatile(qtd_setup.add(0), data_phys); // next → data qTD

    let has_data = data_out.is_some() || data_in.is_some();
    if has_data {
        let (pid, buf_phys, len) = if let Some(out) = data_out {
            (TOKEN_PID_OUT, out.as_ptr() as u32, out.len() as u32)
        } else if let Some(inn) = &data_in {
            (TOKEN_PID_IN, inn.as_ptr() as u32, inn.len() as u32)
        } else { unreachable!() };

        let data_len = len.min(4096);
        write_volatile(qtd_data.add(0), 1u32); // next = terminate
        write_volatile(qtd_data.add(1), 1u32); // alt = terminate
        let data_token = TOKEN_ACTIVE | pid | (data_len << 16) | TOKEN_IOC;
        write_volatile(qtd_data.add(2), data_token);
        write_volatile(qtd_data.add(3), buf_phys);

        // Link: setup → data
        write_volatile(qtd_setup.add(0), data_phys);
        // Link: data → status
        let status_phys = addr_of!(CTRL_QTD_STATUS) as u32;
        write_volatile(qtd_data.add(0), status_phys);
    } else {
        // No data stage: setup → status directly
        let status_phys = addr_of!(CTRL_QTD_STATUS) as u32;
        write_volatile(qtd_setup.add(0), status_phys);
    }

    // Status stage
    // Direction reverses for status:
    // If data was IN → status is OUT, if data was OUT → status is IN
    let status_pid = if data_in.is_some() { TOKEN_PID_OUT } else { TOKEN_PID_IN };
    write_volatile(qtd_status.add(0), 1u32);
    write_volatile(qtd_status.add(1), 1u32);
    let status_token = TOKEN_ACTIVE | status_pid | (0 << 16) | TOKEN_IOC;
    write_volatile(qtd_status.add(2), status_token);

    // Link QH to async list: set overlay
    let setup_phys = addr_of!(CTRL_QTD_SETUP) as u32;
    write_volatile(qh.add(3), setup_phys); // overlay next qTD = setup qTD
    write_volatile(qh.add(4), 0u32);
    write_volatile(qh.add(5), setup_token);
    write_volatile(qh.add(6), setup_buf_phys);

    // Make sure QH is linked after head in async list
    let qh_phys = addr_of!(CTRL_QH) as u32;
    write_volatile(addr_of!(ASYNC_HEAD_QH) as *mut u32,
                   (qh_phys) | 2 | 0); // bit 1 = QH, bit 0 = 0 (not head)

    // Prime async schedule by writing ASYNCLISTADDR again (doorbell)
    // Actually just need to advance doorbell if available. For async, after linking
    // we need to toggle the doorbell in USBCMD.
    let Some(ref ehci) = EHCI else { return false };
    let op = ehci.op_base;
    let cmd = mmio_read(op, CMD);
    mmio_write(op, CMD, cmd | (1 << 6)); // IAAD (Interrupt on Async Advance)
    mmio_write(op, CMD, cmd | CMD_ASYNC_ENABLE);

    // Wait for completion
    for _ in 0..1000000 {
        let _token = read_volatile(qtd_setup.add(2));
        let _token2 = if has_data { read_volatile(qtd_data.add(2)) } else { 0 };
        let token3 = read_volatile(qtd_status.add(2));
        if token3 & TOKEN_ACTIVE == 0 { break; }
        core::hint::spin_loop();
    }

    // Check error
    let token3 = read_volatile(qtd_status.add(2));
    if token3 & TOKEN_ERROR != 0 {
        return false;
    }

    true
}

// ---------------------------------------------------------------------------
// Poll interrupt transfers
// ---------------------------------------------------------------------------

/// Опрос interrupt transfers (HID keyboard + mouse).
pub unsafe fn poll_interrupts() {
    // ── Keyboard ──
    let kbd_qtd = addr_of!(INT_QTD) as *mut u32;
    let kbd_token = read_volatile(kbd_qtd.add(2));
    if kbd_token & TOKEN_ACTIVE == 0 {
        if kbd_token & TOKEN_ERROR == 0 {
            for d in &USB_DEVICES {
                if d.valid && d.state == DevState::Ready && d.device_class == CLASS_HID && d.subclass == 1 {
                    let mut report = [0u8; 8];
                    report.copy_from_slice(&d.buf[..8]);
                    hid::process_report(&report);
                    break;
                }
            }
        }
        let t = TOKEN_ACTIVE | TOKEN_PID_IN | (8 << 16) | TOKEN_IOC;
        write_volatile(kbd_qtd.add(2), t);
        write_volatile(addr_of!(INT_QH) as *mut u32, 1u32);
        let qh = addr_of!(INT_QH) as *mut u32;
        write_volatile(qh.add(5), t);
    }

    // ── Mouse ──
    let mouse_qtd = addr_of!(INT_QTD_MOUSE) as *mut u32;
    let mouse_token = read_volatile(mouse_qtd.add(2));
    if mouse_token & TOKEN_ACTIVE == 0 {
        if mouse_token & TOKEN_ERROR == 0 {
            for d in &USB_DEVICES {
                if d.valid && d.state == DevState::Ready && d.device_class == CLASS_HID && d.subclass == 2 {
                    let mut report = [0u8; 3];
                    report.copy_from_slice(&d.buf[..3]);
                    hid::process_mouse_report(&report);
                    break;
                }
            }
        }
        let t = TOKEN_ACTIVE | TOKEN_PID_IN | (8 << 16) | TOKEN_IOC;
        write_volatile(mouse_qtd.add(2), t);
        let qh_m = addr_of!(INT_QH_MOUSE) as *mut u32;
        write_volatile(qh_m.add(5), t);
    }
}
