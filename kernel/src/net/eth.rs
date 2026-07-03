//! Ethernet / ARP / IPv4 / UDP / TCP — minimal protocol stack.
//! All operations are blocking/polling; no interrupts.

use crate::net::{nic, OUR_MAC, OUR_IP, ip_checksum, ip_eq};

// ── Ethernet ──────────────────────────────────────────────────────────────
const ETH_TYPE_ARP: u16 = 0x0806;
const ETH_TYPE_IP: u16 = 0x0800;

/// Build an Ethernet frame in `buf` and transmit it.
/// `dst_mac`: destination MAC, `ethertype`: in host byte order.
pub unsafe fn send_eth(dst_mac: &[u8; 6], ethertype: u16, payload: &[u8]) -> bool {
    let mut pkt = [0u8; 1514];
    let total = 14 + payload.len();
    if total > pkt.len() { return false; }
    // dst MAC
    for i in 0..6 { pkt[i] = dst_mac[i]; }
    // src MAC
    for i in 0..6 { pkt[6 + i] = OUR_MAC[i]; }
    // EtherType
    pkt[12] = (ethertype >> 8) as u8;
    pkt[13] = (ethertype & 0xFF) as u8;
    // payload
    for i in 0..payload.len() { pkt[14 + i] = payload[i]; }
    nic::send(&pkt, total)
}

/// Broadcast Ethernet frame.
pub unsafe fn send_eth_broadcast(ethertype: u16, payload: &[u8]) -> bool {
    send_eth(&[0xFF; 6], ethertype, payload)
}

// ── ARP ───────────────────────────────────────────────────────────────────
const ARP_HTYPE_ETH: u16 = 1;
const ARP_PTYPE_IP: u16 = 0x0800;
const ARP_REQUEST: u16 = 1;
const ARP_REPLY: u16 = 2;

#[derive(Clone, Copy)]
struct ArpEntry {
    ip: [u8; 4],
    mac: [u8; 6],
    valid: bool,
}
const MAX_ARP: usize = 16;
static mut ARP_CACHE: [ArpEntry; MAX_ARP] = [ArpEntry { ip: [0; 4], mac: [0; 6], valid: false }; MAX_ARP];

/// Send ARP request: who-has `target_ip`? Broadcast.
pub unsafe fn arp_request(target_ip: &[u8; 4]) {
    let mut pkt = [0u8; 28];
    // Hardware type: Ethernet
    pkt[0] = (ARP_HTYPE_ETH >> 8) as u8; pkt[1] = (ARP_HTYPE_ETH & 0xFF) as u8;
    // Protocol type: IPv4
    pkt[2] = (ARP_PTYPE_IP >> 8) as u8; pkt[3] = (ARP_PTYPE_IP & 0xFF) as u8;
    // Hardware size: 6, Protocol size: 4
    pkt[4] = 6; pkt[5] = 4;
    // Opcode: request
    pkt[6] = (ARP_REQUEST >> 8) as u8; pkt[7] = (ARP_REQUEST & 0xFF) as u8;
    // Sender MAC
    for i in 0..6 { pkt[8 + i] = OUR_MAC[i]; }
    // Sender IP
    for i in 0..4 { pkt[14 + i] = OUR_IP[i]; }
    // Target MAC (zero)
    // Target IP
    for i in 0..4 { pkt[24 + i] = target_ip[i]; }
    send_eth_broadcast(ETH_TYPE_ARP, &pkt);
}

/// Send ARP reply to `target_mac` for `target_ip` (who wants our IP).
pub unsafe fn arp_reply(target_mac: &[u8; 6], target_ip: &[u8; 4]) {
    let mut pkt = [0u8; 28];
    pkt[0] = (ARP_HTYPE_ETH >> 8) as u8; pkt[1] = (ARP_HTYPE_ETH & 0xFF) as u8;
    pkt[2] = (ARP_PTYPE_IP >> 8) as u8; pkt[3] = (ARP_PTYPE_IP & 0xFF) as u8;
    pkt[4] = 6; pkt[5] = 4;
    pkt[6] = (ARP_REPLY >> 8) as u8; pkt[7] = (ARP_REPLY & 0xFF) as u8;
    for i in 0..6 { pkt[8 + i] = OUR_MAC[i]; }
    for i in 0..4 { pkt[14 + i] = OUR_IP[i]; }
    for i in 0..6 { pkt[18 + i] = target_mac[i]; }
    for i in 0..4 { pkt[24 + i] = target_ip[i]; }
    send_eth(target_mac, ETH_TYPE_ARP, &pkt);
}

/// Add/replace an ARP cache entry.
pub unsafe fn arp_cache_add(ip: &[u8; 4], mac: &[u8; 6]) {
    for i in 0..MAX_ARP {
        if ARP_CACHE[i].valid && ip_eq(&ARP_CACHE[i].ip, ip) {
            ARP_CACHE[i].mac = *mac;
            return;
        }
    }
    // Find empty slot (or evict oldest)
    for i in 0..MAX_ARP {
        if !ARP_CACHE[i].valid {
            ARP_CACHE[i].ip = *ip;
            ARP_CACHE[i].mac = *mac;
            ARP_CACHE[i].valid = true;
            return;
        }
    }
    // Evict first
    for i in 1..MAX_ARP { ARP_CACHE[i - 1] = ARP_CACHE[i]; }
    ARP_CACHE[MAX_ARP - 1] = ArpEntry { ip: *ip, mac: *mac, valid: true };
}

/// Look up a MAC in ARP cache.
pub fn arp_cache_lookup(ip: &[u8; 4]) -> Option<[u8; 6]> {
    for i in 0..MAX_ARP {
        if unsafe { ARP_CACHE[i].valid } && ip_eq(&unsafe { ARP_CACHE[i].ip }, ip) {
            return Some(unsafe { ARP_CACHE[i].mac });
        }
    }
    None
}

/// Handle an incoming ARP packet.
pub unsafe fn handle_arp(pkt: &[u8]) {
    if pkt.len() < 28 { return; }
    let op = u16::from_be_bytes([pkt[6], pkt[7]]);
    let src_ip = [pkt[14], pkt[15], pkt[16], pkt[17]];
    let src_mac = [pkt[8], pkt[9], pkt[10], pkt[11], pkt[12], pkt[13]];
    let target_ip = [pkt[24], pkt[25], pkt[26], pkt[27]];

    // Always cache sender info
    arp_cache_add(&src_ip, &src_mac);

    if op == ARP_REQUEST && ip_eq(&target_ip, &OUR_IP) {
        arp_reply(&src_mac, &src_ip);
    }
}

/// Resolve an IP to MAC (blocking, with ARP request + retries).
pub unsafe fn arp_resolve(ip: &[u8; 4], timeout_ticks: u64) -> Option<[u8; 6]> {
    if let Some(mac) = arp_cache_lookup(ip) {
        return Some(mac);
    }
    // Send ARP request (up to 3 tries)
    for _ in 0..3 {
        arp_request(ip);
        let start = crate::syscall::get_tick_count();
        loop {
            if crate::syscall::get_tick_count().wrapping_sub(start) >= timeout_ticks { break; }
            // Poll RX
            let mut buf = [0u8; 1518];
            let n = nic::poll_rx(&mut buf);
            if n > 0 { handle_rx(&buf[..n]); }
            if let Some(mac) = arp_cache_lookup(ip) {
                return Some(mac);
            }
        }
    }
    None
}

// ── IPv4 ──────────────────────────────────────────────────────────────────
const IP_PROTO_UDP: u8 = 17;
const IP_PROTO_TCP: u8 = 6;

/// Build an IPv4 header + payload in `buf`. Returns total length.
/// `payload` does NOT include the IP header (20 bytes prepended).
pub fn build_ipv4(src_ip: &[u8; 4], dst_ip: &[u8; 4], proto: u8, payload_len: usize, id: u16) -> ([u8; 1500], usize) {
    let total = 20 + payload_len;
    let mut hdr = [0u8; 1500];
    hdr[0] = 0x45; // Version=4, IHL=5 (20 bytes)
    hdr[1] = 0;    // DSCP + ECN
    hdr[2] = (total >> 8) as u8;
    hdr[3] = (total & 0xFF) as u8;
    hdr[4] = (id >> 8) as u8;
    hdr[5] = (id & 0xFF) as u8;
    hdr[6] = 0x40; // Don't Fragment
    hdr[7] = 0;
    hdr[8] = 64;   // TTL
    hdr[9] = proto;
    hdr[10] = 0; hdr[11] = 0; // checksum (filled below)
    for i in 0..4 { hdr[12 + i] = src_ip[i]; }
    for i in 0..4 { hdr[16 + i] = dst_ip[i]; }
    // Checksum
    let csum = ip_checksum(&hdr[..20]);
    hdr[10] = (csum >> 8) as u8;
    hdr[11] = (csum & 0xFF) as u8;
    (hdr, total)
}

/// Send an IPv4 packet to `dst_ip`. Resolves MAC via ARP if needed.
pub unsafe fn send_ipv4(dst_ip: &[u8; 4], proto: u8, payload: &[u8], id: u16) -> bool {
    let total_len = 20 + payload.len();
    let mut pkt = [0u8; 1514];
    // IPv4 header
    pkt[0] = 0x45;
    pkt[1] = 0;
    pkt[2] = (total_len >> 8) as u8;
    pkt[3] = (total_len & 0xFF) as u8;
    pkt[4] = (id >> 8) as u8;
    pkt[5] = (id & 0xFF) as u8;
    pkt[6] = 0x40; // DF
    pkt[7] = 0;
    pkt[8] = 64;  // TTL
    pkt[9] = proto;
    pkt[10] = 0; pkt[11] = 0; // checksum
    for i in 0..4 { pkt[12 + i] = OUR_IP[i]; }
    for i in 0..4 { pkt[16 + i] = dst_ip[i]; }
    // Checksum
    let csum = ip_checksum(&pkt[..20]);
    pkt[10] = (csum >> 8) as u8;
    pkt[11] = (csum & 0xFF) as u8;
    // Payload
    for i in 0..payload.len() { pkt[20 + i] = payload[i]; }

    // Resolve MAC
    let target_mac = if ip_eq(dst_ip, &[255; 4]) {
        [0xFF; 6]
    } else if let Some(mac) = arp_resolve(dst_ip, 10) {
        mac
    } else {
        return false;
    };

    // Ethernet frame
    let mut eth = [0u8; 1518];
    for i in 0..6 { eth[i] = target_mac[i]; }
    for i in 0..6 { eth[6 + i] = OUR_MAC[i]; }
    eth[12] = (ETH_TYPE_IP >> 8) as u8;
    eth[13] = (ETH_TYPE_IP & 0xFF) as u8;
    for i in 0..total_len { eth[14 + i] = pkt[i]; }
    nic::send(&eth, 14 + total_len)
}

/// Handle an incoming IPv4 packet.
pub unsafe fn handle_ipv4(pkt: &[u8]) {
    if pkt.len() < 20 { return; }
    if (pkt[0] >> 4) != 4 { return; } // IPv4 only
    let ihl = (pkt[0] & 0x0F) as usize * 4;
    if ihl < 20 || pkt.len() < ihl { return; }
    // Verify checksum
    let csum = u16::from_be_bytes([pkt[10], pkt[11]]);
    let mut check = [0u8; 20];
    for i in 0..20 { check[i] = pkt[i]; }
    check[10] = 0; check[11] = 0;
    if ip_checksum(&check[..ihl]) != csum { return; }

    let total_len = u16::from_be_bytes([pkt[2], pkt[3]]) as usize;
    if pkt.len() < total_len { return; }
    let proto = pkt[9];
    let src_ip = [pkt[12], pkt[13], pkt[14], pkt[15]];
    // dst_ip at pkt[16..20]

    match proto {
        IP_PROTO_UDP => handle_udp(&pkt[ihl..total_len], &src_ip),
        IP_PROTO_TCP => handle_tcp(&pkt[ihl..total_len], &src_ip),
        _ => {}
    }
}

// ── UDP ───────────────────────────────────────────────────────────────────
/// UDP receive callback type.
type UdpCallback = unsafe fn(data: &[u8], src_ip: &[u8; 4], src_port: u16);

const MAX_UDP_HANDLERS: usize = 4;
#[derive(Clone, Copy)]
struct UdpHandler {
    port: u16,
    callback: UdpCallback,
}
static mut UDP_HANDLERS: [Option<UdpHandler>; MAX_UDP_HANDLERS] = [None; MAX_UDP_HANDLERS];

/// Register a UDP handler for a given port.
pub unsafe fn udp_listen(port: u16, cb: UdpCallback) -> bool {
    for i in 0..MAX_UDP_HANDLERS {
        if UDP_HANDLERS[i].is_none() {
            UDP_HANDLERS[i] = Some(UdpHandler { port, callback: cb });
            return true;
        }
    }
    false
}

/// Send a UDP datagram.
pub unsafe fn send_udp(dst_ip: &[u8; 4], dst_port: u16, src_port: u16, payload: &[u8]) -> bool {
    let udp_len = 8 + payload.len();
    let mut udp = [0u8; 1500];
    udp[0] = (src_port >> 8) as u8;
    udp[1] = (src_port & 0xFF) as u8;
    udp[2] = (dst_port >> 8) as u8;
    udp[3] = (dst_port & 0xFF) as u8;
    udp[4] = (udp_len >> 8) as u8;
    udp[5] = (udp_len & 0xFF) as u8;
    udp[6] = 0; udp[7] = 0; // checksum (optional for UDP)
    for i in 0..payload.len() { udp[8 + i] = payload[i]; }

    send_ipv4(dst_ip, IP_PROTO_UDP, &udp[..udp_len], 0)
}

fn handle_udp(data: &[u8], src_ip: &[u8; 4]) {
    if data.len() < 8 { return; }
    let src_port = u16::from_be_bytes([data[2], data[3]]);
    let dst_port = u16::from_be_bytes([data[0], data[1]]);
    let len = u16::from_be_bytes([data[4], data[5]]) as usize;
    if data.len() < len { return; }
    let payload = &data[8..len];

    unsafe {
        for i in 0..MAX_UDP_HANDLERS {
            if let Some(h) = &UDP_HANDLERS[i] {
                if h.port == dst_port {
                    (h.callback)(payload, src_ip, src_port);
                }
            }
        }
    }
}

// ── TCP (minimal — single connection) ────────────────────────────────────
const TCP_FLAG_FIN: u8 = 0x01;
const TCP_FLAG_SYN: u8 = 0x02;
const TCP_FLAG_RST: u8 = 0x04;
const TCP_FLAG_PSH: u8 = 0x08;
const TCP_FLAG_ACK: u8 = 0x10;

pub struct TcpConn {
    pub state: u8,      // 0=CLOSED, 1=SYN_SENT, 2=ESTABLISHED, 3=FIN_WAIT
    pub dst_ip: [u8; 4],
    pub dst_port: u16,
    pub src_port: u16,
    pub seq: u32,
    pub ack: u32,
    pub rxbuf: [u8; 4096],
    pub rxlen: usize,
}

pub const TCP_CLOSED: u8 = 0;
pub const TCP_SYN_SENT: u8 = 1;
pub const TCP_ESTABLISHED: u8 = 2;
pub const TCP_FIN_WAIT: u8 = 3;

pub static mut TCP_CONN: TcpConn = TcpConn {
    state: 0,
    dst_ip: [0; 4],
    dst_port: 0,
    src_port: 0,
    seq: 0,
    ack: 0,
    rxbuf: [0; 4096],
    rxlen: 0,
};

/// Get received TCP data (copies into user buffer, resets rxlen).
pub unsafe fn tcp_read_data(buf: &mut [u8]) -> usize {
    let n = TCP_CONN.rxlen.min(buf.len());
    for i in 0..n { buf[i] = TCP_CONN.rxbuf[i]; }
    TCP_CONN.rxlen = 0;
    n
}

/// Check if TCP connection is established.
pub fn tcp_is_connected() -> bool {
    unsafe { TCP_CONN.state == TCP_ESTABLISHED }
}

/// Get TCP state.
pub fn tcp_state() -> u8 {
    unsafe { TCP_CONN.state }
}

/// Build a raw TCP segment in `buf`.
fn build_tcp(seq: u32, ack: u32, flags: u8, payload: &[u8],
             src_port: u16, dst_port: u16) -> ([u8; 1500], usize)
{
    let hdr_len = 20usize;
    let total = hdr_len + payload.len();
    let mut pkt = [0u8; 1500];
    pkt[0] = (src_port >> 8) as u8;
    pkt[1] = (src_port & 0xFF) as u8;
    pkt[2] = (dst_port >> 8) as u8;
    pkt[3] = (dst_port & 0xFF) as u8;
    pkt[4] = (seq >> 24) as u8;
    pkt[5] = (seq >> 16) as u8;
    pkt[6] = (seq >> 8) as u8;
    pkt[7] = (seq & 0xFF) as u8;
    pkt[8] = (ack >> 24) as u8;
    pkt[9] = (ack >> 16) as u8;
    pkt[10] = (ack >> 8) as u8;
    pkt[11] = (ack & 0xFF) as u8;
    pkt[12] = (5 << 4) | 0; // data offset = 5 (20 bytes), reserved
    pkt[13] = flags;
    // Window size: 65535
    pkt[14] = 0xFF;
    pkt[15] = 0xFF;
    pkt[16] = 0; pkt[17] = 0; // checksum (filled later)
    pkt[18] = 0; pkt[19] = 0; // urgent pointer
    for i in 0..payload.len() { pkt[20 + i] = payload[i]; }

    // TCP pseudo-header checksum
    let mut pseudo = [0u8; 12];
    // src IP, dst IP filled by caller
    pseudo[9] = 6; // TCP protocol
    let tcp_len = total as u16;
    pseudo[10] = (tcp_len >> 8) as u8;
    pseudo[11] = (tcp_len & 0xFF) as u8;

    // The checksum is computed over pseudo + TCP header + data
    // We'll compute it in the caller after filling src/dst IP
    (pkt, total)
}

/// Compute TCP checksum (pseudo-header + TCP segment).
fn tcp_checksum(src_ip: &[u8; 4], dst_ip: &[u8; 4], tcp_seg: &[u8]) -> u16 {
    let total = 12 + tcp_seg.len();
    let mut buf = [0u8; 4096];
    if total > buf.len() { return 0; }
    for i in 0..4 { buf[i] = src_ip[i]; }
    for i in 0..4 { buf[4 + i] = dst_ip[i]; }
    buf[8] = 0; buf[9] = 6; // zero + protocol
    let len = tcp_seg.len() as u16;
    buf[10] = (len >> 8) as u8;
    buf[11] = (len & 0xFF) as u8;
    for i in 0..tcp_seg.len() { buf[12 + i] = tcp_seg[i]; }
    // Zero the checksum field in the TCP segment
    let ck_off = 12 + 16;
    buf[ck_off] = 0; buf[ck_off + 1] = 0;
    ip_checksum(&buf[..total])
}

/// Send a TCP segment.
pub unsafe fn send_tcp_raw(conn: &TcpConn, flags: u8, payload: &[u8]) -> bool {
    let (mut seg, total) = build_tcp(conn.seq, conn.ack, flags, payload, conn.src_port, conn.dst_port);
    let csum = tcp_checksum(&OUR_IP, &conn.dst_ip, &seg[..total]);
    seg[16] = (csum >> 8) as u8;
    seg[17] = (csum & 0xFF) as u8;
    send_ipv4(&conn.dst_ip, IP_PROTO_TCP, &seg[..total], 0)
}

/// Initiate a TCP connection.
pub unsafe fn tcp_connect(dst_ip: &[u8; 4], dst_port: u16, src_port: u16) -> bool {
    TCP_CONN.state = TCP_SYN_SENT;
    TCP_CONN.dst_ip = *dst_ip;
    TCP_CONN.dst_port = dst_port;
    TCP_CONN.src_port = src_port;
    TCP_CONN.seq = 1000;
    TCP_CONN.ack = 0;
    TCP_CONN.rxlen = 0;

    send_tcp_raw(&TCP_CONN, TCP_FLAG_SYN, &[]);

    let start = crate::syscall::get_tick_count();
    loop {
        if crate::syscall::get_tick_count().wrapping_sub(start) >= 3000 { break; }
        let mut buf = [0u8; 1518];
        let n = nic::poll_rx(&mut buf);
        if n > 0 { handle_rx(&buf[..n]); }
        if TCP_CONN.state == TCP_ESTABLISHED { return true; }
    }
    TCP_CONN.state = TCP_CLOSED;
    false
}

/// Send data on established TCP connection.
pub unsafe fn tcp_send(data: &[u8]) -> bool {
    if TCP_CONN.state != TCP_ESTABLISHED { return false; }
    send_tcp_raw(&TCP_CONN, TCP_FLAG_PSH | TCP_FLAG_ACK, data);
    true
}

fn handle_tcp(data: &[u8], _src_ip: &[u8; 4]) {
    if data.len() < 20 { return; }
    let seq = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    let ack = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
    let flags = data[13];
    let hdr_len = ((data[12] >> 4) as usize) * 4;
    let payload = if data.len() > hdr_len { &data[hdr_len..] } else { &[] };

    unsafe {
        let conn = &mut TCP_CONN;

        if conn.state == TCP_SYN_SENT && (flags & TCP_FLAG_SYN) != 0 && (flags & TCP_FLAG_ACK) != 0 {
            conn.state = TCP_ESTABLISHED;
            conn.ack = seq.wrapping_add(1);
            conn.seq = ack;
            send_tcp_raw(&conn, TCP_FLAG_ACK, &[]);
            return;
        }

        if conn.state == TCP_ESTABLISHED && (flags & TCP_FLAG_ACK) != 0 {
            if !payload.is_empty() {
                let n = payload.len().min(conn.rxbuf.len().saturating_sub(conn.rxlen));
                for i in 0..n { conn.rxbuf[conn.rxlen + i] = payload[i]; }
                conn.rxlen += n;
                conn.ack = seq.wrapping_add(payload.len() as u32);
                send_tcp_raw(&conn, TCP_FLAG_ACK, &[]);
                return;
            }
            if (flags & TCP_FLAG_FIN) != 0 {
                conn.state = TCP_FIN_WAIT;
                conn.ack = seq.wrapping_add(1);
                send_tcp_raw(&conn, TCP_FLAG_ACK, &[]);
                return;
            }
        }
    }
}

// ── RX dispatch ───────────────────────────────────────────────────────────
/// Handle a raw Ethernet frame from the NIC.
pub unsafe fn handle_rx(frame: &[u8]) {
    if frame.len() < 14 { return; }
    let ethertype = u16::from_be_bytes([frame[12], frame[13]]);
    let payload = &frame[14..];
    match ethertype {
        ETH_TYPE_ARP => handle_arp(payload),
        ETH_TYPE_IP => handle_ipv4(payload),
        _ => {}
    }
}
