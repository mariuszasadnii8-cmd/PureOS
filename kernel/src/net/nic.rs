//! RTL8139 NIC driver — PCI vendor 0x10EC, device 0x8139.
//! Polling mode (no interrupts). DMA buffers in identity-mapped RAM.

use crate::cpu::{inb, inw, inl, outb, outw, outl, pci_read32, pci_write32};
use crate::terminal;

// ── RTL8139 I/O registers (offset from I/O base) ──────────────────────────
const REG_MAC0: u16 = 0x00;  // MAC address bytes 0-3 (32-bit read)
const REG_MAC4: u16 = 0x04;  // MAC address bytes 4-5 (16-bit read)
const REG_CR: u16 = 0x37;    // Command Register
const REG_RCR: u16 = 0x44;   // RX Configuration (32-bit)
const REG_TCR: u16 = 0x40;   // TX Configuration (32-bit)
const REG_IMR: u16 = 0x3C;   // Interrupt Mask Register (16-bit)
const REG_ISR: u16 = 0x3E;   // Interrupt Status Register (16-bit)
const REG_RBSTART: u16 = 0x30; // RX Buffer Start Address (32-bit)
const REG_CAPR: u16 = 0x38;  // Current Address of Packet Read (16-bit)
const REG_CBR: u16 = 0x3A;   // Current Buffer Address (16-bit, read-only)
const REG_TX_STATUS: [u16; 4] = [0x10, 0x14, 0x18, 0x1C];
const REG_TX_ADDR: [u16; 4] = [0x20, 0x24, 0x28, 0x2C];
const REG_TX_COMMAND: [u16; 4] = [0x00, 0x00, 0x00, 0x00]; // TX polls via status

const REG_CONFIG1: u16 = 0x53;

// Command register bits
const CR_RST: u8 = 0x10;
const CR_RE: u8 = 0x08;
const CR_TE: u8 = 0x04;

// RX Configuration bits
const RCR_RBLEN_8K: u32 = 0x0000;
const RCR_RBLEN_16K: u32 = 0x0800;
const RCR_RBLEN_32K: u32 = 0x1000;
const RCR_RBLEN_64K: u32 = 0x1800;
const RCR_AB: u32 = 0x0008; // Accept Broadcast
const RCR_AM: u32 = 0x0004; // Accept Multicast
const RCR_APM: u32 = 0x0002; // Accept Physical Match
const RCR_AAP: u32 = 0x0001; // Accept All Packets

// TX Status bits
const TX_STATUS_OWN: u32 = 0x2000; // DMA operation completed
const TX_STATUS_TOK: u32 = 0x8000; // Transmit OK

// ── Buffer sizes ──────────────────────────────────────────────────────────
const RX_BUF_SIZE: usize = 8192;
const TX_BUF_SIZE: usize = 1792;
const NUM_TX: usize = 4;

/// Our static DMA buffers — must not cross 64K boundary.
const _ALIGN: usize = 16;
static mut DMA: DmaBufs = DmaBufs {
    raw_rx: [0; RX_BUF_SIZE + _ALIGN],
    tx: [[0; TX_BUF_SIZE]; NUM_TX],
    rx_start: 0, rx_cur: 0, tx_idx: 0,
    io_base: 0, initialized: false,
};

struct DmaBufs {
    raw_rx: [u8; RX_BUF_SIZE + _ALIGN],
    tx: [[u8; TX_BUF_SIZE]; NUM_TX],
    rx_start: usize,  // aligned RX start offset in raw_rx
    rx_cur: u16,       // current read offset within RX buffer
    tx_idx: usize,     // next TX descriptor to use
    io_base: u16,
    initialized: bool,
}

/// Initialize the RTL8139 NIC. Scans PCI for the device, sets up DMA buffers,
/// enables RX/TX. Returns true if successful.
pub unsafe fn init() -> bool {
    // ── Find RTL8139 on PCI ────────────────────────────────────────────────
    let mut found = false;
    let mut io_base = 0u16;
    let mut bus = 0u8;
    let mut slot = 0u8;
    let mut func = 0u8;

    for b in 0u8..8 {
        for s in 0u8..32 {
            let vd = pci_read32(b, s, 0, 0);
            let v = (vd & 0xFFFF) as u16;
            if v == 0xFFFF { continue; }
            let d = (vd >> 16) as u16;
            // RTL8139: vendor 0x10EC, device 0x8139
            if v == 0x10EC && d == 0x8139 {
                bus = b; slot = s; func = 0;
                io_base = (pci_read32(b, s, 0, 0x10) & 0xFFFE) as u16;
                found = true;
                break;
            }
        }
        if found { break; }
    }

    if !found {
        terminal::write(b"net: RTL8139 not found on PCI\n");
        return false;
    }

    terminal::write(b"net: RTL8139 at ");
    terminal::write_num(bus as u64);
    terminal::write(b":");
    terminal::write_num(slot as u64);
    terminal::write(b".0  I/O 0x");
    terminal::write_num(io_base as u64);
    terminal::write(b"\n");

    // ── Enable bus mastering ───────────────────────────────────────────────
    let cmd = pci_read32(bus, slot, func, 0x04);
    pci_write32(bus, slot, func, 0x04, cmd | 0x07); // I/O, Memory, Bus Master

    // ── Power up the chip ──────────────────────────────────────────────────
    outb(io_base + REG_CONFIG1, 0x00);

    // ── Soft reset ─────────────────────────────────────────────────────────
    outb(io_base + REG_CR, CR_RST);
    let mut timeout = 1000;
    while (inb(io_base + REG_CR) & CR_RST) != 0 && timeout > 0 {
        timeout -= 1;
    }
    if timeout == 0 {
        terminal::write(b"net: RTL8139 reset timeout\n");
        return false;
    }

    // ── Get MAC address ────────────────────────────────────────────────────
    let mac0 = inl(io_base + REG_MAC0);
    let mac4 = inw(io_base + REG_MAC4);
    let mac = [
        (mac0 >> 0) as u8, (mac0 >> 8) as u8,
        (mac0 >> 16) as u8, (mac0 >> 24) as u8,
        (mac4 >> 0) as u8, (mac4 >> 8) as u8,
    ];
    crate::net::OUR_MAC = mac;

    terminal::write(b"net: MAC ");
    for i in 0..6 {
        let hi = mac[i] >> 4;
        let lo = mac[i] & 0x0F;
        let mut h = [0u8; 2];
        h[0] = if hi < 10 { b'0' + hi } else { b'a' + hi - 10 };
        h[1] = if lo < 10 { b'0' + lo } else { b'a' + lo - 10 };
        terminal::write(&h);
        if i < 5 { terminal::write(b":"); }
    }
    terminal::write(b"\n");

    // ── Set up RX buffer ───────────────────────────────────────────────────
    // Align to 16 bytes
    let rx_phys = &DMA.raw_rx as *const u8 as usize;
    let aligned = (rx_phys + 15) & !15;
    DMA.rx_start = aligned - rx_phys;
    DMA.rx_cur = 0;

    // Write RX buffer start address (lower 32 bits of physical address)
    outl(io_base + REG_RBSTART, aligned as u32);

    // ── Configure RX ───────────────────────────────────────────────────────
    let rcr = RCR_AB | RCR_APM | RCR_RBLEN_8K; // accept broadcast + unicast
    outl(io_base + REG_RCR, rcr);

    // ── Enable RX/TX ───────────────────────────────────────────────────────
    outb(io_base + REG_CR, CR_RE | CR_TE);

    // ── Enable interrupts we care about (optional in polling mode) ─────────
    outw(io_base + REG_IMR, 0x0005); // ROK | TOK

    DMA.io_base = io_base;
    DMA.initialized = true;

    terminal::write(b"net: RTL8139 initialized\n");
    true
}

/// Poll for a received Ethernet frame. Copies into `buf`, returns length (0 = none).
pub unsafe fn poll_rx(buf: &mut [u8]) -> usize {
    if !DMA.initialized { return 0; }
    let io = DMA.io_base;
    let capr = inw(io + REG_CAPR) as usize;
    let cbr = inw(io + REG_CBR) as usize;

    if capr == cbr || DMA.rx_cur as usize == cbr {
        return 0; // no new packets
    }

    // Read packet header from RX buffer
    let off = DMA.rx_cur as usize % RX_BUF_SIZE;
    let pkt_len = u16::from_le_bytes([DMA.raw_rx[DMA.rx_start + off + 2], DMA.raw_rx[DMA.rx_start + off + 3]]) as usize;

    if pkt_len < 4 || pkt_len > RX_BUF_SIZE {
        DMA.rx_cur = cbr as u16;
        outw(io + REG_CAPR, DMA.rx_cur.wrapping_sub(16));
        return 0;
    }

    let data_end = off + 4 + pkt_len;
    let pkt_start = off + 4;
    let copy_len = (pkt_len - 4).min(buf.len()); // skip CRC last 4 bytes

    if data_end <= RX_BUF_SIZE {
        for i in 0..copy_len {
            buf[i] = DMA.raw_rx[DMA.rx_start + pkt_start + i];
        }
    } else {
        // Wrapped around — two copies
        let first = RX_BUF_SIZE - pkt_start;
        for i in 0..first.min(copy_len) {
            buf[i] = DMA.raw_rx[DMA.rx_start + pkt_start + i];
        }
        let remaining = copy_len.saturating_sub(first);
        for i in 0..remaining {
            buf[first + i] = DMA.raw_rx[DMA.rx_start + i];
        }
    }

    // Update read pointer
    DMA.rx_cur = (DMA.rx_cur + 4 + pkt_len as u16) % RX_BUF_SIZE as u16;
    outw(io + REG_CAPR, DMA.rx_cur.wrapping_sub(16));

    copy_len
}

/// Send an Ethernet frame. Returns true on success.
pub unsafe fn send(buf: &[u8], len: usize) -> bool {
    if !DMA.initialized || len > TX_BUF_SIZE { return false; }
    let io = DMA.io_base;
    let idx = DMA.tx_idx % NUM_TX;

    // Wait for previous TX to complete
    let mut timeout = 100000;
    while timeout > 0 {
        let status = inl(io + REG_TX_STATUS[idx]);
        if (status & TX_STATUS_OWN) != 0 || (status & TX_STATUS_TOK) != 0 { break; }
        timeout -= 1;
    }

    // Copy data into TX buffer
    for i in 0..len {
        DMA.tx[idx][i] = buf[i];
    }

    // Set TX address (physical address of the buffer)
    let tx_phys = &DMA.tx[idx] as *const u8 as u32;
    outl(io + REG_TX_ADDR[idx], tx_phys);

    // Send
    let cmd = (len as u32 & 0x1FFF) | 0x000F0000; // size + OWN + TOK
    outl(io + REG_TX_STATUS[idx], cmd);

    DMA.tx_idx = (DMA.tx_idx + 1) % NUM_TX;
    true
}
