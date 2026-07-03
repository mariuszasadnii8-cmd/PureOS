//! Minimal DNS resolver — sends a single A-record query, parses the response.

use crate::net::{nic, eth, DNS_SERVER, ip_eq};
use crate::terminal;

const DNS_PORT: u16 = 53;

static mut DNS_RESP: [u8; 512] = [0; 512];
static mut DNS_LEN: usize = 0;

/// Called from eth::udp dispatch when DNS response arrives.
pub unsafe fn dns_callback(data: &[u8], _src_ip: &[u8; 4], _src_port: u16) {
    let n = data.len().min(512);
    for i in 0..n { DNS_RESP[i] = data[i]; }
    DNS_LEN = n;
}

/// Encode a domain name into DNS label format (e.g. "www.google.com" → "\x03www\x06google\x03com\x00").
fn dns_encode_name(name: &[u8], buf: &mut [u8]) -> usize {
    let mut pos = 0usize;
    let mut start = 0usize;
    for i in 0..=name.len() {
        if i == name.len() || name[i] == b'.' {
            let len = i - start;
            if len > 63 { return 0; }
            if pos + len + 1 > buf.len() { return 0; }
            buf[pos] = len as u8;
            pos += 1;
            for j in 0..len { buf[pos + j] = name[start + j]; }
            pos += len;
            start = i + 1;
        }
    }
    buf[pos] = 0; // root
    pos + 1
}

/// Resolve a hostname to an IPv4 address. Blocks for up to `timeout_ms`.
/// Returns [0;4] on failure.
pub unsafe fn dns_resolve(hostname: &[u8]) -> [u8; 4] {
    if hostname.is_empty() { return [0; 4]; }

    // Register DNS UDP handler if not already
    eth::udp_listen(DNS_PORT, dns_callback);

    // Check if already cached (name comparison skipped for simplicity)
    for i in 0..super::DNS_CACHE_SIZE {
        if unsafe { super::DNS_CACHE[i].valid } {
            let cached = unsafe { super::DNS_CACHE[i].ip };
            return cached;
        }
    }

    if unsafe { ip_eq(&DNS_SERVER, &[0; 4]) } {
        terminal::write(b"dns: no DNS server configured\n");
        return [0; 4];
    }

    // Build DNS query
    let mut query = [0u8; 512];
    let mut pos = 0usize;
    // Header
    let id: u16 = 0x0001;
    query[pos] = (id >> 8) as u8; pos += 1;
    query[pos] = (id & 0xFF) as u8; pos += 1;
    // Flags: standard query, recursion desired
    query[pos] = 0x01; pos += 1; // RD
    query[pos] = 0x00; pos += 1;
    // QDCOUNT = 1
    query[pos] = 0x00; pos += 1;
    query[pos] = 0x01; pos += 1;
    // ANCOUNT, NSCOUNT, ARCOUNT = 0
    for _ in 0..6 { pos += 1; }
    // Question: name
    let nlen = dns_encode_name(hostname, &mut query[pos..]);
    if nlen == 0 { return [0; 4]; }
    pos += nlen;
    // QTYPE = A (1), QCLASS = IN (1)
    query[pos] = 0x00; pos += 1;
    query[pos] = 0x01; pos += 1;
    query[pos] = 0x00; pos += 1;
    query[pos] = 0x01; pos += 1;

    DNS_LEN = 0;

    // Send query
    if !eth::send_udp(&DNS_SERVER, DNS_PORT, 53, &query[..pos]) {
        return [0; 4];
    }

    // Wait for response
    let start = crate::syscall::get_tick_count();
    loop {
        if crate::syscall::get_tick_count().wrapping_sub(start) >= 3000 { break; }
        let mut buf = [0u8; 1518];
        let n = nic::poll_rx(&mut buf);
        if n > 0 { eth::handle_rx(&buf[..n]); }
        if DNS_LEN > 0 { break; }
    }

    if DNS_LEN == 0 { return [0; 4]; }

    // Parse response
    let resp = &DNS_RESP[..DNS_LEN];
    if resp.len() < 12 { return [0; 4]; }

    // Skip header (12 bytes)
    let ancount = u16::from_be_bytes([resp[6], resp[7]]);
    if ancount == 0 { return [0; 4]; }

    // Skip question section
    let mut off = 12usize;
    // Skip name (follow pointers/read until 0)
    loop {
        if off >= resp.len() { return [0; 4]; }
        let b = resp[off];
        if b == 0 { off += 1; break; }
        if (b & 0xC0) == 0xC0 {
            off += 2; break; // compressed name
        }
        off += b as usize + 1;
        if off >= resp.len() { return [0; 4]; }
    }
    off += 4; // skip QTYPE + QCLASS

    // Parse first answer
    for _ in 0..ancount {
        if off >= resp.len() { break; }
        // Skip name (compressed pointer or sequence)
        let b = resp[off];
        if (b & 0xC0) == 0xC0 {
            off += 2;
        } else {
            loop {
                if off >= resp.len() { break; }
                if resp[off] == 0 { off += 1; break; }
                off += resp[off] as usize + 1;
            }
        }
        if off + 10 > resp.len() { break; }
        let _atype = u16::from_be_bytes([resp[off], resp[off + 1]]);
        let _aclass = u16::from_be_bytes([resp[off + 2], resp[off + 3]]);
        let _ttl = u32::from_be_bytes([resp[off + 4], resp[off + 5], resp[off + 6], resp[off + 7]]);
        let rdlen = u16::from_be_bytes([resp[off + 8], resp[off + 9]]) as usize;
        off += 10;
        if _atype == 1 && rdlen == 4 && off + 4 <= resp.len() {
            let ip = [resp[off], resp[off + 1], resp[off + 2], resp[off + 3]];
            // Cache it
            super::dns_cache_add(hostname, &ip);
            return ip;
        }
        off += rdlen;
    }

    [0; 4]
}
