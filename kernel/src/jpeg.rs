//! Baseline JPEG decoder (no_std, no alloc).
//! Supports SOF0, 4:4:4/4:2:2/4:2:0 subsampling, 8-bit, single scan.
//! Integer IDCT (separable, 256-scaling), YCbCr→RGB, byte-stuffed streams.

use crate::framebuffer::{self, Rgb};
use crate::terminal;

// ── Zigzag order (JPEG standard) ──────────────────────────────────────────
const ZIGZAG: [u8; 64] = [
     0,  1,  8, 16,  9,  2,  3, 10,
    17, 24, 32, 25, 18, 11,  4,  5,
    12, 19, 26, 33, 40, 48, 41, 34,
    27, 20, 13,  6,  7, 14, 21, 28,
    35, 42, 49, 56, 57, 50, 43, 36,
    29, 22, 15, 23, 30, 37, 44, 51,
    58, 59, 52, 45, 38, 31, 39, 46,
    53, 60, 61, 54, 47, 55, 62, 63,
];

// ── 1D-IDCT matrix: M[i][k] = round(0.5 * C(k) * cos((2i+1)kπ/16) * 256) ─
// C(0) = 1/√2, C(k>0) = 1.
// For k=0: 0.5 * 0.7071 * 256 ≈ 91
// For k>0: 0.5 * 1 * 256 * cos(angle) = 128 * cos(angle)
// cos values precomputed as COS128[n] = round(128 * cos(n*π/16))
const COS128: [i32; 32] = [
    128, 126, 118, 106, 91, 71, 49, 25, 0, -25, -49, -71, -91, -106, -118, -126,
    -128, -126, -118, -106, -91, -71, -49, -25, 0, 25, 49, 71, 91, 106, 118, 126,
];
const M0: i32 = 91; // M[i][0] for all i

fn idct_1d_row(coeff: &[i32; 8]) -> [i32; 8] {
    let mut out = [0i32; 8];
    for i in 0..8 {
        let mut sum = coeff[0] * M0;
        for k in 1..8 {
            let angle = ((2 * i + 1) * k) % 32;
            sum += coeff[k] * COS128[angle as usize];
        }
        out[i] = sum / 256;
    }
    out
}

/// 2D IDCT on an 8x8 block (dequantised coefficients in natural order).
/// Returns 64 pixel values (0..255, level-shifted).
fn idct_2d(block: &[i32; 64]) -> [u8; 64] {
    // Row transform
    let mut tmp = [0i32; 64];
    for y in 0..8 {
        let mut row = [0i32; 8];
        for x in 0..8 { row[x] = block[y * 8 + x]; }
        let trow = idct_1d_row(&row);
        for x in 0..8 { tmp[y * 8 + x] = trow[x]; }
    }
    // Column transform
    let mut out = [0u8; 64];
    for x in 0..8 {
        let mut col = [0i32; 8];
        for y in 0..8 { col[y] = tmp[y * 8 + x]; }
        let tcol = idct_1d_row(&col);
        for y in 0..8 {
            let val = tcol[y] + 128;
            out[y * 8 + x] = if val < 0 { 0 } else if val > 255 { 255 } else { val as u8 };
        }
    }
    out
}

// ── Bit reader (MSB-first, byte-stuffed) ──────────────────────────────────
struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    buf: u32,
    bits: u8,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        BitReader { data, pos: 0, buf: 0, bits: 0 }
    }

    fn fill(&mut self) {
        while self.bits <= 24 && self.pos < self.data.len() {
            let mut b = self.data[self.pos] as u32;
            self.pos += 1;
            // Byte stuffing: 0xFF 0x00 → 0xFF (skip 0x00)
            if b == 0xFF && self.pos < self.data.len() && self.data[self.pos] == 0x00 {
                self.pos += 1;
            }
            // RST markers are handled by caller — here we just skip them
            if b == 0xFF && self.pos < self.data.len() && (self.data[self.pos] & 0xF8) == 0xD0 {
                // RST0..RST7 — reset DC prediction, skip this marker
                self.pos += 1;
                continue;
            }
            self.buf = (self.buf << 8) | b;
            self.bits += 8;
        }
    }

    fn peek_bits(&mut self, n: u8) -> u32 {
        self.fill();
        (self.buf >> (self.bits - n)) & ((1u32 << n) - 1)
    }

    fn skip_bits(&mut self, n: u8) {
        if n >= self.bits {
            self.bits = 0;
            self.buf = 0;
        } else {
            self.bits -= n;
            self.buf &= (1u32 << self.bits) - 1;
        }
    }

    fn read_bits(&mut self, n: u8) -> u32 {
        let val = self.peek_bits(n);
        self.skip_bits(n);
        val
    }

    fn read_bit(&mut self) -> u32 {
        self.read_bits(1)
    }
}

// ── Huffman table (canonical JPEG format) ──────────────────────────────────
struct HuffTable {
    max_code: [i32; 16], // largest code of each length (or -1)
    min_code: [i32; 16], // smallest code of each length (or -1)
    val_ptr: [u8; 16],   // start index in values[]
    values: [u8; 256],
}

impl HuffTable {
    fn empty() -> Self {
        HuffTable { max_code: [-1; 16], min_code: [-1; 16], val_ptr: [0; 16], values: [0; 256] }
    }

    fn decode(&self, br: &mut BitReader) -> u8 {
        let mut code = 0i32;
        for l in 0..16 {
            code = (code << 1) | br.read_bit() as i32;
            if code <= self.max_code[l] && code >= self.min_code[l] {
                let idx = code - self.min_code[l];
                return self.values[self.val_ptr[l] as usize + idx as usize];
            }
        }
        0
    }
}

// ── JPEG decoder state ────────────────────────────────────────────────────
pub unsafe fn display_jpeg_file(path: &[u8]) {
    let node = match crate::fs::resolve(path) {
        Some(n) if crate::fs::kind(n) == crate::fs::Kind::File => n,
        _ => {
            terminal::write(b"jpeg: file not found: ");
            terminal::write(path);
            terminal::write(b"\n");
            return;
        }
    };
    let data = crate::fs::read(node);
    if decode_jpeg(data) {
        terminal::write(b"Image displayed: ");
        terminal::write(path);
        terminal::write(b"\n");
    } else {
        terminal::write(b"jpeg: unsupported or invalid JPEG: ");
        terminal::write(path);
        terminal::write(b"\n");
    }
}

/// Decode a JPEG in memory and render to framebuffer at (0,0).
pub fn decode_jpeg(data: &[u8]) -> bool {
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return false; // not a valid JPEG (no SOI)
    }

    // ── Parse markers ──────────────────────────────────────────────────────
    let mut pos = 2usize;
    let mut width = 0u16;
    let mut height = 0u16;
    let mut num_components = 0u8;
    let mut comp_id = [0u8; 3];
    let mut comp_h = [1u8; 3];
    let mut comp_v = [1u8; 3];
    let mut comp_tq = [0u8; 3];
    let mut comp_td = [0u8; 3];
    let mut comp_ta = [0u8; 3];
    let mut quant_tables = [[0u16; 64]; 4];
    let mut quant_present = [false; 4];
    let mut dc_tables: [HuffTable; 4] = [
        HuffTable::empty(), HuffTable::empty(),
        HuffTable::empty(), HuffTable::empty(),
    ];
    let mut ac_tables: [HuffTable; 4] = [
        HuffTable::empty(), HuffTable::empty(),
        HuffTable::empty(), HuffTable::empty(),
    ];
    let mut scan_start = 0usize;

    loop {
        if pos + 1 > data.len() { return false; }
        if data[pos] != 0xFF {
            // Some JPEGs may have padding before EOI — bail if no marker
            if pos >= data.len() - 1 { break; }
            pos += 1;
            continue;
        }
        pos += 1;
        if pos >= data.len() { return false; }
        let marker = data[pos];
        pos += 1;

        match marker {
            0xD8 => {} // SOI — ignore
            0xD9 => { break; } // EOI
            0x00 => {} // stuffed byte
            0xD0..=0xD7 => {} // RST — ignore, handled in bit reader
            0xE0..=0xEF | 0xFE => {
                // APPn / COM — skip
                if pos + 1 > data.len() { return false; }
                let seg_len = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
                if pos + seg_len > data.len() { return false; }
                pos += seg_len - 2;
            }
            0xDB => {
                // DQT — quantization table
                if pos + 1 > data.len() { return false; }
                let seg_len = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
                if pos + seg_len > data.len() { return false; }
                let end = pos + seg_len;
                pos += 2; // skip length
                while pos < end {
                    if pos >= data.len() { return false; }
                    let info = data[pos];
                    pos += 1;
                    let prec = (info >> 4) & 0x0F;
                    let tqi = (info & 0x0F) as usize;
                    if tqi >= 4 { return false; }
                    let mut qvals = [0u16; 64];
                    if prec == 0 {
                        // 8-bit
                        for i in 0..64 {
                            if pos >= data.len() { return false; }
                            qvals[ZIGZAG[i] as usize] = data[pos] as u16;
                            pos += 1;
                        }
                    } else {
                        // 16-bit
                        for i in 0..64 {
                            if pos + 1 >= data.len() { return false; }
                            qvals[ZIGZAG[i] as usize] = u16::from_be_bytes([data[pos], data[pos + 1]]);
                            pos += 2;
                        }
                    }
                    quant_tables[tqi] = qvals;
                    quant_present[tqi] = true;
                }
            }
            0xC0 => {
                // SOF0 — baseline frame header
                if pos + 1 > data.len() { return false; }
                let seg_len = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
                if pos + seg_len > data.len() { return false; }
                if seg_len < 6 { return false; }
                let precision = data[pos + 2];
                if precision != 8 { return false; }
                height = u16::from_be_bytes([data[pos + 3], data[pos + 4]]);
                width = u16::from_be_bytes([data[pos + 5], data[pos + 6]]);
                num_components = data[pos + 7];
                if num_components != 3 { return false; } // only YCbCr
                if seg_len < 8 + (num_components as usize) * 3 { return false; }
                for i in 0..num_components as usize {
                    let base = pos + 8 + i * 3;
                    comp_id[i] = data[base];
                    comp_h[i] = (data[base + 1] >> 4) & 0x0F;
                    comp_v[i] = data[base + 1] & 0x0F;
                    comp_tq[i] = data[base + 2];
                    if comp_h[i] == 0 || comp_v[i] == 0 { return false; }
                    if comp_tq[i] as usize >= 4 { return false; }
                    if !quant_present[comp_tq[i] as usize] { return false; }
                }
                pos += seg_len - 2;
            }
            0xC4 => {
                // DHT — Huffman table
                if pos + 1 > data.len() { return false; }
                let seg_len = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
                if pos + seg_len > data.len() { return false; }
                let end = pos + seg_len;
                pos += 2;
                while pos < end {
                    if pos >= data.len() { return false; }
                    let info = data[pos];
                    pos += 1;
                    let is_ac = (info >> 4) & 0x0F;
                    let table_id = (info & 0x0F) as usize;
                    if table_id >= 4 { return false; }
                    // Read bits[16]
                    let mut bits = [0u8; 16];
                    let mut num_symbols = 0usize;
                    for i in 0..16 {
                        if pos >= data.len() { return false; }
                        bits[i] = data[pos];
                        num_symbols += bits[i] as usize;
                        pos += 1;
                    }
                    // Read values (the symbols themselves)
                    let mut values = [0u8; 256];
                    for i in 0..num_symbols.min(256) {
                        if pos >= data.len() { return false; }
                        values[i] = data[pos];
                        pos += 1;
                    }
                    // Build table
                    if is_ac == 0 {
                        dc_tables[table_id].fill_from_bits(&bits, &values);
                    } else {
                        ac_tables[table_id].fill_from_bits(&bits, &values);
                    }
                }
            }
            0xDA => {
                // SOS — start of scan
                if pos + 1 > data.len() { return false; }
                let seg_len = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
                if pos + seg_len > data.len() { return false; }
                let num_scan_comp = data[pos + 2] as usize;
                if num_scan_comp != num_components as usize { return false; }
                for i in 0..num_scan_comp {
                    let base = pos + 3 + i * 2;
                    for j in 0..num_components as usize {
                        if data[base] == comp_id[j] {
                            comp_td[j] = (data[base + 1] >> 4) & 0x0F;
                            comp_ta[j] = data[base + 1] & 0x0F;
                        }
                    }
                }
                let spectral_start = data[pos + 3 + num_scan_comp * 2];
                let spectral_end = data[pos + 4 + num_scan_comp * 2];
                let approx = data[pos + 5 + num_scan_comp * 2];
                if spectral_start != 0 || spectral_end != 63 || approx != 0 {
                    return false; // only single-scan baseline
                }
                let _header_len = 2 + seg_len;
                scan_start = pos + seg_len - 2; // skip SOS segment
                pos = data.len(); // break out of loop
            }
            _ => {
                // Unknown marker — skip by length
                if pos + 1 > data.len() { return false; }
                let seg_len = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
                if pos + seg_len > data.len() { return false; }
                pos += seg_len - 2;
            }
        }
    }

    if width == 0 || height == 0 || scan_start == 0 { return false; }

    // ── Decode scan data ───────────────────────────────────────────────────
    let mut br = BitReader::new(&data[scan_start..]);

    // Find max sampling factors
    let h_max = (comp_h[0]).max(comp_h[1]).max(comp_h[2]);
    let v_max = (comp_v[0]).max(comp_v[1]).max(comp_v[2]);
    let mcu_w = 8 * h_max as u16;
    let mcu_h = 8 * v_max as u16;

    let mcus_x = ((width as u32 + mcu_w as u32 - 1) / mcu_w as u32) as u16;
    let mcus_y = ((height as u32 + mcu_h as u32 - 1) / mcu_h as u32) as u16;

    // Per-component block counts per MCU
    let mut blocks_per_mcu = [0usize; 3];
    let mut total_blocks = 0usize;
    for i in 0..num_components as usize {
        let n = (comp_h[i] as usize) * (comp_v[i] as usize);
        blocks_per_mcu[i] = n;
        total_blocks += n;
    }

    // Temporary buffers per component per MCU
    // max blocks per component = 4 (4:2:0 Y)
    let mut comp_buf: [[[u8; 64]; 4]; 3] = [[[0; 64]; 4]; 3];
    let mut dc_pred = [0i32; 3];

    let fb_w = framebuffer::width();
    let fb_h = framebuffer::height();

    // Pre-allocate 8x8 coefficients
    let mut coeff = [0i32; 64];

    for mcu_y_idx in 0..mcus_y {
        for mcu_x_idx in 0..mcus_x {
            let base_y = (mcu_y_idx as u32) * (mcu_h as u32);
            let base_x = (mcu_x_idx as u32) * (mcu_w as u32);

            // Decode each component's blocks
            for ci in 0..num_components as usize {
                let h = comp_h[ci] as usize;
                let v = comp_v[ci] as usize;
                let tq = comp_tq[ci] as usize;
                let td = comp_td[ci] as usize;
                let ta = comp_ta[ci] as usize;

                for by in 0..v {
                    for bx in 0..h {
                        let bi = by * h + bx;

                        // ── Decode DC coefficient ─────────────────────────
                        {
                            let cat = dc_tables[td].decode(&mut br) as u8;
                            if cat > 11 { return false; }
                            let mut diff = 0i32;
                            if cat > 0 {
                                let bits = br.read_bits(cat);
                                // If MSB = 0, it's negative (extend sign)
                                if (bits >> (cat - 1)) & 1 == 0 {
                                    diff = bits as i32 - ((1u32 << cat) - 1) as i32;
                                } else {
                                    diff = bits as i32;
                                }
                            }
                            dc_pred[ci] += diff;
                            coeff[0] = dc_pred[ci];
                        }

                        // ── Decode AC coefficients ─────────────────────────
                        for k in 1..64 { coeff[k] = 0; }
                        let mut k = 1usize;
                        while k < 64 {
                            let sym = ac_tables[ta].decode(&mut br);
                            if sym == 0 { break; } // EOB
                            let run = (sym >> 4) as usize;
                            let size = sym & 0x0F;
                            if size == 0 { // ZRL (16 zeros)
                                k += 16;
                                continue;
                            }
                            k += run;
                            if k >= 64 { break; }
                            let bits = br.read_bits(size);
                            let ac_val;
                            if (bits >> (size - 1)) & 1 == 0 {
                                ac_val = bits as i32 - ((1u32 << size) - 1) as i32;
                            } else {
                                ac_val = bits as i32;
                            }
                            coeff[k] = ac_val;
                            k += 1;
                        }

                        // ── Dequantize ─────────────────────────────────────
                        let qt = &quant_tables[tq];
                        for i in 0..64 {
                            coeff[i] = coeff[i] * qt[i] as i32;
                        }

                        // ── IDCT and store ────────────────────────────────
                        let pixels = idct_2d(&coeff);
                        comp_buf[ci][bi] = pixels;
                    }
                }
            }

            // ── Render MCU to framebuffer ─────────────────────────────────
            for y_off in 0..(mcu_h as u32) {
                let abs_y = base_y + y_off;
                if abs_y >= fb_h || abs_y >= height as u32 { continue; }

                // Map MCU pixel to component pixel
                for x_off in 0..(mcu_w as u32) {
                    let abs_x = base_x + x_off;
                    if abs_x >= fb_w || abs_x >= width as u32 { continue; }

                    // For each component, find which sub-block and which pixel within it
                    let mut y_val = 0i32;
                    let mut cb_val = 0i32;
                    let mut cr_val = 0i32;

                    for ci in 0..num_components as usize {
                        let h = comp_h[ci] as u32;
                        let v = comp_v[ci] as u32;
                        let blk_w = mcu_w as u32 / h; // 8
                        let blk_h = mcu_h as u32 / v; // 8

                        let sub_x = x_off / blk_w;
                        let sub_y = y_off / blk_h;
                        let bx = (sub_x.min(h - 1)) as usize;
                        let by = (sub_y.min(v - 1)) as usize;
                        let bi = by * (h as usize) + bx;

                        let px = (x_off % blk_w) as usize;
                        let py = (y_off % blk_h) as usize;
                        let pi = py * 8 + px;

                        let val = comp_buf[ci][bi][pi] as i32;
                        match ci {
                            0 => y_val = val,
                            1 => cb_val = val,
                            2 => cr_val = val,
                            _ => {}
                        }
                    }

                    // YCbCr → RGB
                    let r = y_val + ((357 * (cr_val - 128)) >> 8);
                    let g = y_val - ((88 * (cb_val - 128)) >> 8) - ((183 * (cr_val - 128)) >> 8);
                    let b = y_val + ((454 * (cb_val - 128)) >> 8);

                    let r = if r < 0 { 0 } else if r > 255 { 255 } else { r as u8 };
                    let g = if g < 0 { 0 } else if g > 255 { 255 } else { g as u8 };
                    let b = if b < 0 { 0 } else if b > 255 { 255 } else { b as u8 };

                    framebuffer::put(abs_x, abs_y, Rgb(r, g, b));
                }
            }
        }
    }

    true
}

impl HuffTable {
    fn fill_from_bits(&mut self, bits: &[u8; 16], values: &[u8]) {
        let mut code = 0i32;
        let mut vi = 0usize;
        for l in 0..16 {
            let n = bits[l] as usize;
            if n == 0 {
                self.max_code[l] = -1;
                self.min_code[l] = -1;
                self.val_ptr[l] = vi as u8;
                continue;
            }
            self.min_code[l] = code;
            self.val_ptr[l] = vi as u8;
            for _ in 0..n {
                if vi < 256 {
                    self.values[vi] = if vi < values.len() { values[vi] } else { 0 };
                    vi += 1;
                }
                code += 1;
            }
            self.max_code[l] = code - 1;
            code <<= 1;
        }
    }
}
