//! DHCP client — obtains IP, gateway, DNS, netmask via UDP broadcast.

use crate::net::{nic, eth, OUR_IP};
use crate::terminal;

const DHCP_SERVER_PORT: u16 = 67;
const DHCP_CLIENT_PORT: u16 = 68;

const DHCP_DISCOVER: u8 = 1;
const DHCP_OFFER: u8 = 2;
const DHCP_REQUEST: u8 = 3;
const DHCP_ACK: u8 = 5;

const DHCP_OPT_SUBNET: u8 = 1;
const DHCP_OPT_ROUTER: u8 = 3;
const DHCP_OPT_DNS: u8 = 6;
const DHCP_OPT_REQ_IP: u8 = 50;
const DHCP_OPT_LEASE: u8 = 51;
const DHCP_OPT_MSG_TYPE: u8 = 53;
const DHCP_OPT_SERVER_ID: u8 = 54;
const DHCP_OPT_END: u8 = 255;

/// Build a DHCP packet in `buf`. Returns the length.
fn build_dhcp(msg_type: u8, req_ip: &[u8; 4], server_id: &[u8; 4], xid: u32) -> ([u8; 548], usize) {
    let mut pkt = [0u8; 548];
    pkt[0] = 1;  // BOOTREQUEST (for client)
    pkt[1] = 1;  // Ethernet
    pkt[2] = 6;  // hlen
    pkt[3] = 0;  // hops
    pkt[4] = (xid >> 24) as u8;
    pkt[5] = (xid >> 16) as u8;
    pkt[6] = (xid >> 8) as u8;
    pkt[7] = (xid & 0xFF) as u8;
    // secs, flags — zero
    // ciaddr (client IP) — zero for DISCOVER
    for i in 0..6 { pkt[28 + i] = unsafe { crate::net::OUR_MAC[i] }; }
    // zero out remaining 10 bytes of client hw addr padding
    // server host name (64 bytes), boot file (128 bytes)
    // Magic cookie
    pkt[236] = 99; pkt[237] = 130; pkt[238] = 83; pkt[239] = 99;
    let mut off = 240usize;
    // DHCP message type
    pkt[off] = DHCP_OPT_MSG_TYPE; pkt[off + 1] = 1; pkt[off + 2] = msg_type;
    off += 3;
    if msg_type == DHCP_REQUEST {
        // Requested IP
        pkt[off] = DHCP_OPT_REQ_IP; pkt[off + 1] = 4;
        for i in 0..4 { pkt[off + 2 + i] = req_ip[i]; }
        off += 6;
        // Server identifier
        pkt[off] = DHCP_OPT_SERVER_ID; pkt[off + 1] = 4;
        for i in 0..4 { pkt[off + 2 + i] = server_id[i]; }
        off += 6;
        // Also request subnet mask, router, DNS
    }
    // Always request subnet mask, router, DNS
    if msg_type == DHCP_DISCOVER || msg_type == DHCP_REQUEST {
        let param_req = [DHCP_OPT_SUBNET, DHCP_OPT_ROUTER, DHCP_OPT_DNS];
        pkt[off] = 55; pkt[off + 1] = 3; // parameter request list
        for i in 0..3 { pkt[off + 2 + i] = param_req[i]; }
        off += 5;
    }
    pkt[off] = DHCP_OPT_END;
    off += 1;
    // Pad to at least 300 bytes (some servers require this)
    while off < 300 {
        pkt[off] = 0;
        off += 1;
    }
    (pkt, off)
}

/// Parse DHCP options and extract config.
fn parse_dhcp_options(data: &[u8]) -> (bool, [u8; 4], [u8; 4], [u8; 4]) {
    let mut yiaddr = [0u8; 4];
    // yiaddr is at offset 16 in the DHCP header
    for i in 0..4 { yiaddr[i] = data[16 + i]; }
    let mut subnet = [255u8; 4];
    let mut router = [0u8; 4];
    let mut dns = [0u8; 4];

    if data.len() < 240 { return (false, yiaddr, router, dns); }
    // Check magic cookie
    if data[236] != 99 || data[237] != 130 || data[238] != 83 || data[239] != 99 {
        return (false, yiaddr, router, dns);
    }
    let mut off = 240usize;
    loop {
        if off >= data.len() { break; }
        let tag = data[off];
        off += 1;
        if tag == DHCP_OPT_END { break; }
        if tag == 0 { continue; } // pad
        if off >= data.len() { break; }
        let len = data[off] as usize;
        off += 1;
        if off + len > data.len() { break; }
        match tag {
            DHCP_OPT_SUBNET => {
                for i in 0..4.min(len) { subnet[i] = data[off + i]; }
            }
            DHCP_OPT_ROUTER => {
                for i in 0..4.min(len) { router[i] = data[off + i]; }
            }
            DHCP_OPT_DNS => {
                for i in 0..4.min(len) { dns[i] = data[off + i]; }
            }
            _ => {}
        }
        off += len;
    }
    (true, yiaddr, router, dns)
}

/// Run DHCP to get an IP address. Returns true on success.
pub unsafe fn dhcp_negotiate() -> bool {
    terminal::write(b"net: DHCP starting...\n");

    let xid: u32 = 0x12345678;
    let mut req_ip = [0u8; 4];
    let mut server_id = [0u8; 4];

    // ── DISCOVER ───────────────────────────────────────────────────────────
    let (disc, dlen) = build_dhcp(DHCP_DISCOVER, &[0; 4], &[0; 4], xid);
    if !eth::send_udp(&[255; 4], DHCP_SERVER_PORT, DHCP_CLIENT_PORT, &disc[..dlen]) {
        terminal::write(b"net: DHCP DISCOVER send failed\n");
        return false;
    }

    // Wait for OFFER
    let start = crate::syscall::get_tick_count();
    let mut offer_yiaddr = [0u8; 4];
    let mut offer_server = [0u8; 4];
    let mut got_offer = false;

    loop {
        if crate::syscall::get_tick_count().wrapping_sub(start) >= 5000 { break; }
        let mut buf = [0u8; 1518];
        let n = nic::poll_rx(&mut buf);
        if n > 0 {
            eth::handle_rx(&buf[..n]);
            // Check if we got an OFFER by parsing UDP callback
            // We use a static variable set by the DHCP UDP handler
            if got_offer { break; }
        }
    }

    if !got_offer {
        terminal::write(b"net: DHCP no OFFER\n");
        return false;
    }

    // ── REQUEST ────────────────────────────────────────────────────────────
    terminal::write(b"net: DHCP OFFER received, sending REQUEST\n");
    req_ip = offer_yiaddr;
    server_id = offer_server;

    let (req, rlen) = build_dhcp(DHCP_REQUEST, &req_ip, &server_id, xid);
    eth::send_udp(&[255; 4], DHCP_SERVER_PORT, DHCP_CLIENT_PORT, &req[..rlen]);

    // Wait for ACK
    let start = crate::syscall::get_tick_count();
    let mut got_ack = false;

    loop {
        if crate::syscall::get_tick_count().wrapping_sub(start) >= 5000 { break; }
        let mut buf = [0u8; 1518];
        let n = nic::poll_rx(&mut buf);
        if n > 0 {
            eth::handle_rx(&buf[..n]);
            if got_ack { break; }
        }
    }

    if !got_ack {
        terminal::write(b"net: DHCP no ACK\n");
        return false;
    }

    // ── Parse final config ─────────────────────────────────────────────────
    terminal::write(b"net: DHCP ACK - IP ");
    let mut ipbuf = [0u8; 16];
    let nl = crate::net::fmt_ip(&OUR_IP, &mut ipbuf);
    terminal::write(&ipbuf[..nl]);
    terminal::write(b"\n");

    true
}
