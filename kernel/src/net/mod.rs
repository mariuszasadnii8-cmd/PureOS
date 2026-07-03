//! PureOS network stack — RTL8139 NIC + IPv4/UDP/TCP + DHCP/DNS/HTTP.
//! QEMU: `-netdev user,id=net0 -device rtl8139,netdev=net0`

pub mod nic;
pub mod eth;
pub mod dhcp;
pub mod dns;
pub mod http;

/// Our MAC address (set by NIC init).
pub(crate) static mut OUR_MAC: [u8; 6] = [0; 6];

/// Our IP address (set by DHCP or static).
pub static mut OUR_IP: [u8; 4] = [0; 4];

/// Gateway IP.
pub static mut GATEWAY: [u8; 4] = [0; 4];

/// DNS server IP.
pub static mut DNS_SERVER: [u8; 4] = [0; 4];

/// Subnet mask.
pub static mut NETMASK: [u8; 4] = [255; 4]; // default: /32

/// Seconds since boot (approx).
pub static mut UPTIME_SECS: u64 = 0;

/// DNS cache.
const DNS_CACHE_SIZE: usize = 8;
#[derive(Clone, Copy)]
struct DnsEntry {
    valid: bool,
    ip: [u8; 4],
}
pub static mut DNS_CACHE: [DnsEntry; DNS_CACHE_SIZE] = [DnsEntry { valid: false, ip: [0; 4] }; DNS_CACHE_SIZE];

pub unsafe fn dns_cache_add(_name: &[u8], ip: &[u8; 4]) {
    for i in 0..DNS_CACHE_SIZE {
        if !DNS_CACHE[i].valid {
            DNS_CACHE[i].ip = *ip;
            DNS_CACHE[i].valid = true;
            return;
        }
    }
}

/// Call once per second from the scheduler to update net timers.
pub unsafe fn tick_second() {
    UPTIME_SECS = UPTIME_SECS.wrapping_add(1);
}

/// Checksum helpers.
pub fn ip_checksum(buf: &[u8]) -> u16 {
    let mut sum = 0u32;
    let mut i = 0;
    while i + 1 < buf.len() {
        sum += u16::from_be_bytes([buf[i], buf[i + 1]]) as u32;
        i += 2;
    }
    if i < buf.len() {
        sum += (buf[i] as u32) << 8;
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

/// Compare two IPv4 addresses.
pub fn ip_eq(a: &[u8; 4], b: &[u8; 4]) -> bool {
    a[0] == b[0] && a[1] == b[1] && a[2] == b[2] && a[3] == b[3]
}

/// Format an IPv4 address into a buffer. Returns length written.
pub fn fmt_ip(ip: &[u8; 4], buf: &mut [u8]) -> usize {
    let mut n = 0usize;
    for i in 0..4 {
        if i > 0 && n < buf.len() { buf[n] = b'.'; n += 1; }
        let mut d = 100u32;
        let mut printed = false;
        while d > 0 {
            let digit = (ip[i] as u32 / d) % 10;
            if printed || digit > 0 || d == 1 {
                if n < buf.len() { buf[n] = b'0' + digit as u8; n += 1; }
                printed = true;
            }
            d /= 10;
        }
    }
    n
}
