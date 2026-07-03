//! Minimal HTTP/1.1 client — GET requests over TCP.

use crate::net::{nic, eth, dns};
use crate::terminal;

/// Perform an HTTP GET request. `host` is the domain, `path` is the resource path.
/// Blocks until complete or timeout. Writes response to `out` and returns length.
pub unsafe fn http_get(host: &[u8], path: &[u8], out: &mut [u8]) -> usize {
    // Resolve hostname
    let ip = dns::dns_resolve(host);
    if ip == [0; 4] {
        terminal::write(b"http: DNS resolution failed\n");
        return 0;
    }

    terminal::write(b"http: resolved ");
    terminal::write(host);
    terminal::write(b" -> ");
    let mut ipbuf = [0u8; 16];
    let nl = crate::net::fmt_ip(&ip, &mut ipbuf);
    terminal::write(&ipbuf[..nl]);
    terminal::write(b"\n");

    // TCP connect to port 80
    let src_port = 40000;
    if !eth::tcp_connect(&ip, 80, src_port) {
        terminal::write(b"http: TCP connect failed\n");
        return 0;
    }

    terminal::write(b"http: connected, sending GET\n");

    // Send HTTP GET request
    let mut req = [0u8; 1024];
    let mut pos = 0usize;
    let get = b"GET ";
    for i in 0..4 { req[pos + i] = get[i]; } pos += 4;
    for i in 0..path.len() { req[pos + i] = path[i]; } pos += path.len();
    let sp = b" HTTP/1.1\r\nHost: ";
    for i in 0..14 { req[pos + i] = sp[i]; } pos += 14;
    for i in 0..host.len() { req[pos + i] = host[i]; } pos += host.len();
    let crlf = b"\r\nConnection: close\r\n\r\n";
    for i in 0..22 { req[pos + i] = crlf[i]; } pos += 22;

    if !eth::tcp_send(&req[..pos]) {
        terminal::write(b"http: TCP send failed\n");
        return 0;
    }

    terminal::write(b"http: request sent, waiting for response\n");

    // Wait for response data
    let start = crate::syscall::get_tick_count();
    let mut total = 0usize;
    loop {
        if crate::syscall::get_tick_count().wrapping_sub(start) >= 10000 { break; }
        let mut buf = [0u8; 1518];
        let n = nic::poll_rx(&mut buf);
        if n > 0 { eth::handle_rx(&buf[..n]); }

        // Copy any received TCP data
        let n = eth::tcp_read_data(&mut out[total..]);
        total += n;
        if eth::tcp_state() == eth::TCP_FIN_WAIT { break; }
    }

    terminal::write(b"http: done, ");
    terminal::write_num(total as u64);
    terminal::write(b" bytes\n");
    total
}
