//! GIF87a/89a decoder — no_std, no alloc.
//! Baseline: first-frame display + animated viewer (APIC-driven, key‑to‑exit).
//! LZW decompression with a 4096‑entry static dictionary.

use crate::framebuffer::{self, Rgb};
use crate::terminal;

const MAX_BITS: u8 = 12;
const DICT_N: usize = 1 << MAX_BITS; // 4096

// ── Color table ───────────────────────────────────────────────────────────
struct ColorTable {
    entries: [[u8; 3]; 256],
    n: u8,
}
impl ColorTable {
    fn empty() -> Self { ColorTable { entries: [[0; 3]; 256], n: 0 } }

    fn parse(&mut self, raw: &[u8], count: usize) {
        let c = count.min(256);
        self.n = c as u8;
        for i in 0..c {
            let off = i * 3;
            if off + 2 < raw.len() {
                self.entries[i] = [raw[off], raw[off + 1], raw[off + 2]];
            }
        }
    }

    fn get(&self, idx: u8) -> Rgb {
        let i = (idx as usize) % (self.n.max(1) as usize);
        Rgb(self.entries[i][0], self.entries[i][1], self.entries[i][2])
    }
}

// ── Sub‑block bit reader (LSB‑first) ──────────────────────────────────────
struct GifReader<'a> {
    data: &'a [u8],
    pos: usize,
    buf: u32,
    bits: u8,
    left: u8,
}
impl<'a> GifReader<'a> {
    fn new(data: &'a [u8], mut pos: usize) -> Self {
        let mut r = GifReader { data, pos, buf: 0, bits: 0, left: 0 };
        r.next_block();
        r
    }
    fn next_block(&mut self) {
        if self.pos < self.data.len() {
            self.left = self.data[self.pos];
            self.pos += 1;
        } else { self.left = 0; }
    }
    fn fill(&mut self) {
        while self.bits <= 24 && self.left > 0 && self.pos < self.data.len() {
            let b = self.data[self.pos] as u32;
            self.pos += 1;
            self.left -= 1;
            self.buf |= b << self.bits;
            self.bits += 8;
            if self.left == 0 { self.next_block(); }
        }
    }
    fn read(&mut self, n: u8) -> u32 {
        self.fill();
        let v = self.buf & ((1u32 << n) - 1);
        self.buf >>= n;
        self.bits = self.bits.saturating_sub(n);
        v
    }
}

// ── LZW decompressor ──────────────────────────────────────────────────────
struct Lzw {
    prefix: [u16; DICT_N],
    suffix: [u8; DICT_N],        // last byte of the string
    stack: [u8; DICT_N],
    next: u16,                   // next free dictionary slot
    code_sz: u8,
    clear: u16,
    end: u16,
    min_c: u8,
}
impl Lzw {
    fn new(min_c: u8) -> Self {
        let clear = 1u16 << (min_c as u16);
        let end = clear + 1;
        let mut l = Lzw {
            prefix: [0; DICT_N], suffix: [0; DICT_N], stack: [0; DICT_N],
            next: end + 1, code_sz: min_c + 1, clear, end, min_c,
        };
        for i in 0..(clear as usize) { l.suffix[i] = i as u8; }
        l
    }
    fn reset(&mut self) {
        let clear = 1u16 << (self.min_c as u16);
        self.next = clear + 2; // clear + end + 1 = first available
        self.code_sz = self.min_c + 1;
        self.clear = clear;
        self.end = clear + 1;
        for i in 0..(clear as usize) { self.suffix[i] = i as u8; }
    }

    /// Return first byte of the string represented by `code`.
    fn first_byte(&self, code: u16) -> u8 {
        let mut c = code;
        while c > self.clear { c = self.prefix[c as usize]; }
        c as u8
    }

    /// Decompress LZW data from `rd` into `out`. Returns bytes written.
    fn decompress(&mut self, rd: &mut GifReader, out: &mut [u8]) -> usize {
        let mut written = 0usize;
        let mut old = 0u16;

        // ── First code ────────────────────────────────────────────────────
        let mut code = self.read_code(rd);
        if code == self.end { return 0; }
        if code < 256 && written < out.len() { out[written] = code as u8; written += 1; }
        old = code;

        loop {
            code = self.read_code(rd);
            if code == self.end { break; }

            if code == self.clear {
                self.reset();
                code = self.read_code(rd);
                if code == self.end { break; }
                if code < 256 && written < out.len() { out[written] = code as u8; written += 1; }
                old = code;
                continue;
            }

            // ── Build output string onto stack (reversed) ─────────────────
            let mut sp = 0usize;
            let mut c = code;

            if code >= self.next {
                // Special case: CODE == next_entry (not yet in dictionary)
                // The string is dict[old] + first_byte(dict[old])
                // Push first_byte(dict[old]) first, then walk old's chain
                let fb = self.first_byte(old);
                self.stack[sp] = fb;
                sp += 1;
                c = old;
            }

            // Walk chain: push suffix bytes (leaf → root), then the root byte
            while c > self.clear && sp < DICT_N {
                self.stack[sp] = self.suffix[c as usize];
                sp += 1;
                c = self.prefix[c as usize];
            }
            self.stack[sp] = c as u8;
            sp += 1;

            // Pop stack → output in correct order
            let first_out = self.stack[sp - 1]; // first byte of this output
            while sp > 0 && written < out.len() {
                sp -= 1;
                out[written] = self.stack[sp];
                written += 1;
            }

            // ── Add to dictionary ─────────────────────────────────────────
            if self.next < DICT_N as u16 {
                self.prefix[self.next as usize] = old;
                self.suffix[self.next as usize] = first_out;
                self.next += 1;
                if self.next > (1u16 << self.code_sz) && self.code_sz < MAX_BITS {
                    self.code_sz += 1;
                }
            }

            old = code;
        }
        written
    }

    fn read_code(&mut self, rd: &mut GifReader) -> u16 {
        rd.read(self.code_sz) as u16
    }
}

// ── Frame metadata ────────────────────────────────────────────────────────
#[derive(Clone, Copy)]
struct Frame {
    left: u16, top: u16, w: u16, h: u16,
    interlace: bool,
    transparent: bool,
    tindex: u8,
    disposal: u8,
    delay_cs: u16,
    lzw_min: u8,
    data_off: u32,      // offset of LZW minimum‑code byte
    has_local: bool,
    local_off: u32,     // offset of local colour table (−1 = none)
    local_n: u8,        // number of local CT entries
}

const MAX_FRAMES: usize = 256;

// ── Public API ────────────────────────────────────────────────────────────
pub unsafe fn display_gif_file(path: &[u8]) {
    let node = match crate::fs::resolve(path) {
        Some(n) if crate::fs::kind(n) == crate::fs::Kind::File => n,
        _ => { terminal::write(b"gif: file not found\n"); return; }
    };
    let data = crate::fs::read(node);
    if !decode_gif(data) {
        terminal::write(b"gif: unsupported/invalid\n");
    }
}

pub unsafe fn decode_gif(data: &[u8]) -> bool {
    if data.len() < 6 { return false; }
    if &data[..6] != b"GIF87a" && &data[..6] != b"GIF89a" { return false; }
    if data.len() < 13 { return false; }

    let w = u16::from_le_bytes([data[6], data[7]]);
    let h = u16::from_le_bytes([data[8], data[9]]);
    let pk = data[10];
    let has_gct = (pk & 0x80) != 0;
    let gct_n = 1 << ((pk & 0x07) + 1);
    let bg = data[11];

    let mut gct = ColorTable::empty();
    let mut pos = 13usize;
    if has_gct {
        let n = gct_n as usize * 3;
        if pos + n > data.len() { return false; }
        gct.parse(&data[pos..], gct_n as usize);
        pos += n;
    }

    // ── First pass: collect frames ────────────────────────────────────────
    let mut frames: [Frame; MAX_FRAMES] = [Frame {
        left:0, top:0, w:0, h:0, interlace:false,
        transparent:false, tindex:0, disposal:0, delay_cs:0,
        lzw_min:0, data_off:0, has_local:false, local_off:0, local_n:0,
    }; MAX_FRAMES];
    let mut fcnt = 0usize;

    let mut tr = false;
    let mut ti = 0u8;
    let mut disp = 0u8;
    let mut delay = 0u16;

    loop {
        if pos >= data.len() { break; }
        match data[pos] {
            0x21 => {
                pos += 1; if pos >= data.len() { break; }
                let label = data[pos]; pos += 1;
                loop {
                    if pos >= data.len() { break; }
                    let sz = data[pos] as usize;
                    pos += 1;
                    if sz == 0 { break; }
                    if pos + sz > data.len() { break; }
                    if label == 0xF9 && sz >= 4 {
                        let p = data[pos];
                        disp = (p >> 2) & 7;
                        tr = (p & 1) != 0;
                        delay = u16::from_le_bytes([data[pos+1], data[pos+2]]);
                        ti = data[pos+3];
                    }
                    pos += sz;
                }
            }
            0x2C => {
                pos += 1;
                if pos + 8 > data.len() { break; }
                let il = u16::from_le_bytes([data[pos], data[pos+1]]);
                let it = u16::from_le_bytes([data[pos+2], data[pos+3]]);
                let iw = u16::from_le_bytes([data[pos+4], data[pos+5]]);
                let ih = u16::from_le_bytes([data[pos+6], data[pos+7]]);
                let ipk = data[pos+8];
                let inter = (ipk & 0x40) != 0;
                let has_local = (ipk & 0x80) != 0;
                let local_n = if has_local { 1 << ((ipk & 7) + 1) } else { 0 };
                let local_off = if has_local { (pos + 9) as u32 } else { 0 };
                pos += 9;

                if has_local {
                    let n = local_n as usize * 3;
                    if pos + n > data.len() { break; }
                    pos += n;
                }
                if pos >= data.len() { break; }
                let lzw_min = data[pos];
                let data_off = pos as u32;
                pos += 1;

                // Skip sub‑blocks
                loop {
                    if pos >= data.len() { break; }
                    let sb = data[pos] as usize;
                    pos += 1;
                    if sb == 0 { break; }
                    if pos + sb > data.len() { break; }
                    pos += sb;
                }

                if fcnt < MAX_FRAMES {
                    frames[fcnt] = Frame {
                        left:il, top:it, w:iw, h:ih,
                        interlace:inter,
                        transparent:tr, tindex:ti,
                        disposal:disp, delay_cs:delay,
                        lzw_min, data_off,
                        has_local, local_off, local_n: local_n as u8,
                    };
                    fcnt += 1;
                }
            }
            0x3B => { break; }
            _ => { pos += 1; }
        }
    }

    if fcnt == 0 { return false; }

    // ── Render first frame ────────────────────────────────────────────────
    render_frame(data, &frames[0], &gct, w, h);

    // ── Animation loop ────────────────────────────────────────────────────
    if fcnt > 1 {
        terminal::write(b"GIF: ");
        terminal::write_num(fcnt as u64);
        terminal::write(b" frames. Any key exits.\n");

        let mut fi = 1usize;
        let mut pd = frames[0].disposal;
        let (mut pl, mut pt, mut pw, mut ph) = (
            frames[0].left, frames[0].top, frames[0].w, frames[0].h);

        loop {
            if crate::keyboard::read_key().is_some() { break; }
            let f = &frames[fi];

            if f.delay_cs > 0 {
                let ms = f.delay_cs as u64 * 10;
                let t0 = unsafe { crate::syscall::get_tick_count() };
                loop {
                    if crate::keyboard::read_key().is_some() { break; }
                    let now = unsafe { crate::syscall::get_tick_count() };
                    if now.wrapping_sub(t0) >= ms { break; }
                }
            }

            // Apply previous disposal
            match pd {
                2 | 3 => {
                    let bgc = if has_gct && (bg as usize) < gct.n as usize { gct.get(bg) }
                              else { Rgb(0,0,0) };
                    for y in pt..pt+ph {
                        for x in pl..pl+pw {
                            framebuffer::put(x as u32, y as u32, bgc);
                        }
                    }
                }
                _ => {}
            }
            (pl, pt, pw, ph) = (f.left, f.top, f.w, f.h);
            render_frame(data, f, &gct, w, h);
            pd = f.disposal;

            fi += 1;
            if fi >= fcnt { fi = 0; }
        }
        terminal::write(b"GIF done.\n");
    }
    true
}

// ── Render one frame ──────────────────────────────────────────────────────
fn render_frame(data: &[u8], f: &Frame, gct: &ColorTable, _sw: u16, _sh: u16) {
    let _ = _sw;
    let _ = _sh;
    let fb_w = framebuffer::width();
    let fb_h = framebuffer::height();

    // Colour table
    let mut lct = ColorTable::empty();
    let ct: &ColorTable = if f.has_local {
        let off = f.local_off as usize;
        let n = f.local_n as usize * 3;
        if off + n <= data.len() {
            lct.parse(&data[off..], f.local_n as usize);
            &lct
        } else { gct }
    } else { gct };

    // LZW decompress
    let start = f.data_off as usize + 1; // past LZW min‑code byte
    if start > data.len() { return; }
    let mut rd = GifReader::new(data, start);
    let mut lzw = Lzw::new(f.lzw_min);

    let mut pixels = [0u8; 65536];
    let n = lzw.decompress(&mut rd, &mut pixels);

    // Interlace: reorder rows
    if f.interlace {
        let mut reorder = [0u8; 65536];
        let mut out_y = 0usize;
        for pass in 0..4 {
            let (start_row, step) = match pass {
                0 => (0u16, 8u16),
                1 => (4, 8),
                2 => (2, 4),
                3 => (1, 2),
                _ => (0, 1),
            };
            let mut r = start_row;
            while r < f.h && out_y < n {
                let src_off = out_y * f.w as usize;
                let dst_off = r as usize * f.w as usize;
                let ncpy = (f.w as usize).min(pixels.len().saturating_sub(dst_off))
                                          .min(pixels.len().saturating_sub(src_off));
                for i in 0..ncpy {
                    reorder[dst_off + i] = pixels[src_off + i];
                }
                out_y += 1;
                r += step;
            }
        }
        pixels = reorder;
    }

    // Blit
    for y in 0..f.h {
        let ay = f.top as u32 + y as u32;
        if ay >= fb_h { continue; }
        for x in 0..f.w {
            let ax = f.left as u32 + x as u32;
            if ax >= fb_w { continue; }
            let idx = pixels[y as usize * f.w as usize + x as usize];
            if f.transparent && idx == f.tindex { continue; }
            framebuffer::put(ax, ay, ct.get(idx));
        }
    }
}
