//! BMP image decoder for PureOS.
//! Supports: BMP v3, 24-bit uncompressed (BI_RGB).
//! Zero-Alloc: renders directly to framebuffer.

use crate::framebuffer::{self, Rgb};
use crate::terminal;

/// Parsed BMP info
pub struct BmpInfo {
    pub width: u32,
    pub height: u32,
    pub data_offset: u32,
    pub valid: bool,
}

/// Minimal BMP header parser (v3, 24-bit).
pub fn parse_header(data: &[u8]) -> BmpInfo {
    if data.len() < 54 {
        return BmpInfo { width: 0, height: 0, data_offset: 0, valid: false };
    }
    // bfType must be 'BM' (0x4D42)
    if data[0] != 0x42 || data[1] != 0x4D {
        return BmpInfo { width: 0, height: 0, data_offset: 0, valid: false };
    }
    let data_offset = u32::from_le_bytes([data[10], data[11], data[12], data[13]]);
    let width = u32::from_le_bytes([data[18], data[19], data[20], data[21]]);
    let height_raw = i32::from_le_bytes([data[22], data[23], data[24], data[25]]);
    let bit_count = u16::from_le_bytes([data[28], data[29]]);
    let compression = u32::from_le_bytes([data[30], data[31], data[32], data[33]]);

    if bit_count != 24 || compression != 0 {
        return BmpInfo { width: 0, height: 0, data_offset: 0, valid: false };
    }

    let height = height_raw.unsigned_abs();
    BmpInfo { width, height, data_offset, valid: true }
}

/// Render a BMP to the framebuffer at (origin_x, origin_y).
/// Returns true on success.
pub fn render_bmp(data: &[u8], origin_x: u32, origin_y: u32) -> bool {
    let info = parse_header(data);
    if !info.valid {
        return false;
    }

    let fb_w = framebuffer::width();
    let fb_h = framebuffer::height();
    if origin_x >= fb_w || origin_y >= fb_h {
        return false;
    }

    let w = info.width.min(fb_w - origin_x);
    let h = info.height.min(fb_h - origin_y);

    // BMP rows are bottom-up, BGR, padded to 4 bytes
    let stride = ((w * 3 + 3) / 4) * 4;

    for y in 0..h {
        let src_row = (info.height - 1 - y) as usize; // bottom-up
        let src_off = info.data_offset as usize + src_row * stride as usize;
        if src_off + (w as usize * 3) > data.len() {
            break;
        }
        for x in 0..w {
            let px_off = src_off + x as usize * 3;
            let b = data[px_off];
            let g = data[px_off + 1];
            let r = data[px_off + 2];
            framebuffer::put(origin_x + x, origin_y + y, Rgb(r, g, b));
        }
    }
    true
}

/// Same as display_bmp_file but returns bool (for wallpaper loader).
pub unsafe fn display_bmp_file_full(path: &[u8]) -> bool {
    let node = match crate::fs::resolve(path) {
        Some(n) if crate::fs::kind(n) == crate::fs::Kind::File => n,
        _ => return false,
    };
    let data = crate::fs::read(node);
    render_bmp(data, 0, 0)
}

/// Display a BMP from a file in the filesystem.
pub unsafe fn display_bmp_file(path: &[u8]) {
    let node = match crate::fs::resolve(path) {
        Some(n) if crate::fs::kind(n) == crate::fs::Kind::File => n,
        _ => {
            terminal::write(b"imgview: file not found: ");
            terminal::write(path);
            terminal::write(b"\n");
            return;
        }
    };
    let data = crate::fs::read(node);
    if render_bmp(data, 0, 0) {
        terminal::write(b"Image displayed: ");
        terminal::write(path);
        terminal::write(b"\n");
    } else {
        terminal::write(b"imgview: unsupported or invalid BMP: ");
        terminal::write(path);
        terminal::write(b"\n");
    }
}

#[cfg(test)]
pub fn test_bmp_header() {
    // Minimal valid BMP header (24x24, 24-bit)
    let mut hdr = [0u8; 54];
    hdr[0] = 0x42; hdr[1] = 0x4D; // 'BM'
    // bfSize = 54 + padding (just header)
    hdr[10] = 54; // bfOffBits
    hdr[18] = 24; // width
    hdr[22] = 24; // height
    hdr[26] = 1;  // planes
    hdr[28] = 24; // bit count
    // compression = 0 (already zero)

    let info = parse_header(&hdr);
    assert!(info.valid);
    assert_eq!(info.width, 24);
    assert_eq!(info.height, 24);
}
