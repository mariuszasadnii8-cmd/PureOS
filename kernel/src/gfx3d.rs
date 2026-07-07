//! 3D графика — программный рендеринг для PureOS.
//!
//! Треугольный растеризатор, backface culling, painter's algorithm.
//! Демки: solid cube, donut, sphere, earth, wireframe додекаэдр.

use crate::framebuffer;
use crate::framebuffer::Rgb;
use crate::keyboard;
use crate::cpu;
use crate::math;

// ═══════════════════════════════════════════════════════════════════
// Вектор / Матрица
// ═══════════════════════════════════════════════════════════════════

#[derive(Clone, Copy)]
pub struct Vec3 { pub x: f32, pub y: f32, pub z: f32 }

pub fn v(x: f32, y: f32, z: f32) -> Vec3 { Vec3 { x, y, z } }

pub unsafe fn rot_x(vv: Vec3, a: f32) -> Vec3 {
    let (c, s) = (math::cos(a), math::sin(a));
    v(vv.x, vv.y * c - vv.z * s, vv.y * s + vv.z * c)
}
pub unsafe fn rot_y(vv: Vec3, a: f32) -> Vec3 {
    let (c, s) = (math::cos(a), math::sin(a));
    v(vv.x * c + vv.z * s, vv.y, -vv.x * s + vv.z * c)
}
pub unsafe fn rot_z(vv: Vec3, a: f32) -> Vec3 {
    let (c, s) = (math::cos(a), math::sin(a));
    v(vv.x * c - vv.y * s, vv.x * s + vv.y * c, vv.z)
}

pub fn dot(a: Vec3, b: Vec3) -> f32 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

pub fn sub(a: Vec3, b: Vec3) -> Vec3 {
    v(a.x - b.x, a.y - b.y, a.z - b.z)
}

pub fn cross(a: Vec3, b: Vec3) -> Vec3 {
    v(a.y * b.z - a.z * b.y, a.z * b.x - a.x * b.z, a.x * b.y - a.y * b.x)
}

pub unsafe fn normalize(vv: Vec3) -> Vec3 {
    let len = math::sqrt(vv.x * vv.x + vv.y * vv.y + vv.z * vv.z);
    if len > 0.0 { v(vv.x / len, vv.y / len, vv.z / len) } else { vv }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 { a + (b - a) * t }

// ═══════════════════════════════════════════════════════════════════
// Растеризация
// ═══════════════════════════════════════════════════════════════════

fn draw_line(x0: i32, y0: i32, x1: i32, y1: i32, color: Rgb) {
    let (w, h) = (framebuffer::width() as i32, framebuffer::height() as i32);
    let (mut x, mut y) = (x0, y0);
    let (dx, sx) = ((x1 - x0).abs(), if x0 < x1 { 1 } else { -1 });
    let (dy, sy) = ((y1 - y0).abs(), if y0 < y1 { 1 } else { -1 });
    let mut e = dx - dy;
    loop {
        if x >= 0 && x < w && y >= 0 && y < h { framebuffer::put(x as u32, y as u32, color); }
        if x == x1 && y == y1 { break; }
        let e2 = 2 * e;
        if e2 > -dy { e -= dy; x += sx; }
        if e2 < dx { e += dx; y += sy; }
    }
}

/// Заливка треугольника (scanline). Сортировка вершин по Y, затем
/// для каждого scanline находим пересечения с двумя рёбрами и заполняем.
fn fill_triangle(
    x0: f32, y0: f32, x1: f32, y1: f32, x2: f32, y2: f32, color: Rgb,
) {
    let (w, h) = (framebuffer::width() as i32, framebuffer::height() as i32);
    // Sort by Y
    let mut pts = [(x0, y0), (x1, y1), (x2, y2)];
    if pts[0].1 > pts[1].1 { pts.swap(0, 1); }
    if pts[0].1 > pts[2].1 { pts.swap(0, 2); }
    if pts[1].1 > pts[2].1 { pts.swap(1, 2); }

    let (x0, y0) = pts[0]; let (x1, y1) = pts[1]; let (x2, y2) = pts[2];
    let y_start = y0 as i32;
    let y_end = y2 as i32;

    for y in y_start..=y_end {
        if y < 0 || y >= h { continue; }
        let fy = y as f32;

        // Left/right edges: compute x intersections
        // Edge 0->1 and 0->2 for upper half, 1->2 and 0->2 for lower half
        let (x_left, x_right) = if fy < y1 {
            // Top half: between edges (0->1) and (0->2)
            let t01 = if (y1 - y0).abs() > 0.01 { (fy - y0) / (y1 - y0) } else { 0.0 };
            let t02 = if (y2 - y0).abs() > 0.01 { (fy - y0) / (y2 - y0) } else { 0.0 };
            (lerp(x0, x1, t01), lerp(x0, x2, t02))
        } else {
            // Bottom half: between edges (1->2) and (0->2)
            let t12 = if (y2 - y1).abs() > 0.01 { (fy - y1) / (y2 - y1) } else { 0.0 };
            let t02 = if (y2 - y0).abs() > 0.01 { (fy - y0) / (y2 - y0) } else { 0.0 };
            (lerp(x1, x2, t12), lerp(x0, x2, t02))
        };

        let (mut xl, mut xr) = (x_left as i32, x_right as i32);
        if xl > xr { core::mem::swap(&mut xl, &mut xr); }
        let xl = xl.max(0).min(w - 1);
        let xr = xr.max(0).min(w - 1);

        for x in xl..=xr {
            framebuffer::put(x as u32, y as u32, color);
        }
    }
}

fn project(vv: Vec3, cx: f32, cy: f32, focal: f32, scale: f32) -> (f32, f32) {
    let z = vv.z + 4.0;
    if z <= 0.1 { return (cx, cy); }
    (vv.x * scale * focal / z + cx, vv.y * scale * focal / z + cy)
}

/// Нормаль треугольника (заданного тремя вершинами).
fn tri_normal(a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let ab = sub(b, a);
    let ac = sub(c, a);
    cross(ab, ac)
}

// ═══════════════════════════════════════════════════════════════════
// SOLID CUBE — цветной куб с видимыми гранями
// ═══════════════════════════════════════════════════════════════════

fn cube_verts() -> [Vec3; 8] {
    [v(-1.0,-1.0,-1.0), v( 1.0,-1.0,-1.0), v( 1.0, 1.0,-1.0), v(-1.0, 1.0,-1.0),
     v(-1.0,-1.0, 1.0), v( 1.0,-1.0, 1.0), v( 1.0, 1.0, 1.0), v(-1.0, 1.0, 1.0)]
}

// 6 граней × 2 треугольника
const CUBE_TRIS: [(usize, usize, usize, Rgb); 12] = [
    (0,1,2, Rgb(220,80,80)),  (0,2,3, Rgb(200,60,60)),   // front (z=-1)
    (5,4,7, Rgb(80,220,80)),  (5,7,6, Rgb(60,200,60)),   // back  (z=+1)
    (4,0,3, Rgb(80,80,220)),  (4,3,7, Rgb(60,60,200)),   // left  (x=-1)
    (1,5,6, Rgb(220,220,80)), (1,6,2, Rgb(200,200,60)),  // right (x=+1)
    (3,2,6, Rgb(220,80,220)), (3,6,7, Rgb(200,60,200)),  // top   (y=+1)
    (4,5,1, Rgb(80,220,220)), (4,1,0, Rgb(60,200,200)),  // bottom(y=-1)
];

/// Вращающийся цветной куб (solid, backface culling, painter).
pub unsafe fn cube3d() {
    let (fw, fh) = (framebuffer::width() as f32, framebuffer::height() as f32);
    let (cx, cy) = (fw / 2.0, fh / 2.0);
    let mut angle = 0.0f32;

    loop {
        while let Some(k) = keyboard::read_key() {
            if k == 0x1B { framebuffer::clear(Rgb(0,0,0)); return; }
        }
        framebuffer::clear(Rgb(5, 0, 10));

        // Transform all vertices
        let mut tv = [v(0.0,0.0,0.0); 8];
        for i in 0..8 {
            tv[i] = rot_y(rot_x(cube_verts()[i], angle * 0.7), angle);
        }

        // Sort triangles by depth (painter's algorithm)
        let light = normalize(v(0.3, 0.5, -1.0));
        let mut order: [usize; 12] = [0,1,2,3,4,5,6,7,8,9,10,11];

        // Bubble sort by average Z
        for _ in 0..12 { for j in 0..11 {
            let (a0,a1,a2,_) = CUBE_TRIS[order[j]];
            let (b0,b1,b2,_) = CUBE_TRIS[order[j+1]];
            let za = (tv[a0].z + tv[a1].z + tv[a2].z) / 3.0;
            let zb = (tv[b0].z + tv[b1].z + tv[b2].z) / 3.0;
            if za < zb { order.swap(j, j+1); }
        }}

        for &ti in &order {
            let (i0, i1, i2, base_color) = CUBE_TRIS[ti];
            let (a, b, c) = (tv[i0], tv[i1], tv[i2]);

            // Backface cull
            let normal = tri_normal(a, b, c);
            let view = v(a.x, a.y, a.z);
            if dot(normal, view) >= 0.0 { continue; } // backface

            // Lighting
            let n = normalize(normal);
            let lum = dot(n, light).max(0.2).min(1.0);
            let color = Rgb(
                (base_color.0 as f32 * lum) as u8,
                (base_color.1 as f32 * lum) as u8,
                (base_color.2 as f32 * lum) as u8,
            );

            let (p0x, p0y) = project(a, cx, cy, 300.0, 1.8);
            let (p1x, p1y) = project(b, cx, cy, 300.0, 1.8);
            let (p2x, p2y) = project(c, cx, cy, 300.0, 1.8);

            fill_triangle(p0x, p0y, p1x, p1y, p2x, p2y, color);
        }

        angle += 0.04;
        delay_ms(16);
    }
}

// ═══════════════════════════════════════════════════════════════════
// DONUT — 3D тор с улучшенным освещением
// ═══════════════════════════════════════════════════════════════════

const ZW: usize = 200;
const ZH: usize = 150;
static mut ZBUF: [u16; ZW * ZH] = [0; ZW * ZH];

pub unsafe fn donut() {
    let (fw, fh) = (framebuffer::width(), framebuffer::height());
    let sx = fw / ZW as u32;
    let sy = fh / ZH as u32;

    const N1: usize = 150;
    const N2: usize = 100;
    let mut ct = [0.0f32; N1]; let mut st = [0.0f32; N1];
    let mut cp = [0.0f32; N2]; let mut sp = [0.0f32; N2];
    for i in 0..N1 { let a = i as f32 * 6.2831853 / N1 as f32;
        ct[i] = math::cos(a); st[i] = math::sin(a); }
    for j in 0..N2 { let a = j as f32 * 6.2831853 / N2 as f32;
        cp[j] = math::cos(a); sp[j] = math::sin(a); }

    let (r_major, r_minor) = (2.2f32, 0.9f32);
    let light = normalize(v(0.4, 0.6, -0.7));
    let mut ang = 0.0f32;

    loop {
        while let Some(k) = keyboard::read_key() {
            if k == 0x1B { framebuffer::clear(Rgb(0,0,0)); return; }
        }
        for z in ZBUF.iter_mut() { *z = 0; }

        let (ca, sa) = (math::cos(ang), math::sin(ang));
        let (ca2, sa2) = (math::cos(ang * 0.4), math::sin(ang * 0.4));

        for i in 0..N1 {
            let (rcx, rcy) = (r_minor * ct[i], r_minor * st[i]);
            for j in 0..N2 {
                let x = (r_major + rcx) * cp[j];
                let y = rcy;
                let zz = (r_major + rcx) * sp[j];

                // Normal (pre-rotation)
                let nx = ct[i] * cp[j];
                let ny = st[i];
                let nz = ct[i] * sp[j];

                // Rotate Y
                let rx = x * ca + zz * sa;
                let ry = y;
                let rz = -x * sa + zz * ca;
                let rnx = nx * ca + nz * sa;
                let rny = ny;
                let rnz = -nx * sa + nz * ca;

                // Rotate X
                let rx2 = rx;
                let ry2 = ry * ca2 - rz * sa2;
                let rz2 = ry * sa2 + rz * ca2;
                let rnx2 = rnx;
                let rny2 = rny * ca2 - rnz * sa2;
                let rnz2 = rny * sa2 + rnz * ca2;

                let zcam = rz2 + 5.0;
                if zcam <= 0.1 { continue; }

                let px = (rx2 / zcam * 100.0 + ZW as f32 / 2.0) as i32;
                let py = (ry2 / zcam * 100.0 + ZH as f32 / 2.0) as i32;
                if px < 0 || px >= ZW as i32 || py < 0 || py >= ZH as i32 { continue; }

                let depth = (1.0 / zcam * 65535.0) as u16;
                let idx = py as usize * ZW + px as usize;
                if depth > ZBUF[idx] {
                    ZBUF[idx] = depth;

                    // Diffuse
                    let n = normalize(v(rnx2, rny2, rnz2));
                    let diff = dot(n, light).max(0.0);

                    // Specular (Blinn-Phong)
                    let half = normalize(v(light.x, light.y, light.z - 1.0));
                    let spec_base = dot(n, half).max(0.0);
                    let spec = { let s = spec_base * spec_base; let s = s * s; let s = s * s; s * s * 0.6 };

                    // Position-based color
                    let hue = (j as f32 / N2 as f32 + ang * 0.1) % 1.0;
                    let r = ((hue * 6.0 - 0.0).abs().min(2.0) - 1.0).abs();
                    let g = ((hue * 6.0 - 2.0).abs().min(2.0) - 1.0).abs();
                    let bb = ((hue * 6.0 - 4.0).abs().min(2.0) - 1.0).abs();
                    let r = (1.0 - r.min(1.0)) * 0.8 + 0.2;
                    let g = (1.0 - g.min(1.0)) * 0.8 + 0.2;
                    let b = (1.0 - bb.min(1.0)) * 0.8 + 0.2;

                    let final_r = ((diff * r + spec) * 255.0).min(255.0) as u8;
                    let final_g = ((diff * g + spec) * 255.0).min(255.0) as u8;
                    let final_b = ((diff * b + spec) * 255.0).min(255.0) as u8;

                    framebuffer::fill_rect(
                        (px as u32) * sx, (py as u32) * sy, sx, sy,
                        Rgb(final_r, final_g, final_b),
                    );
                }
            }
        }
        ang += 0.04;
        delay_ms(16);
    }
}

// ═══════════════════════════════════════════════════════════════════
// SPHERE — вращающаяся сфера (lat/lon квады)
// ═══════════════════════════════════════════════════════════════════

pub unsafe fn sphere() {
    let (fw, fh) = (framebuffer::width() as f32, framebuffer::height() as f32);
    let (cx, cy) = (fw / 2.0, fh / 2.0);

    const NLAT: usize = 20;
    const NLON: usize = 30;

    let mut sin_lat = [0.0f32; NLAT]; let mut cos_lat = [0.0f32; NLAT];
    let mut sin_lon = [0.0f32; NLON]; let mut cos_lon = [0.0f32; NLON];

    for i in 0..NLAT { let a = i as f32 * 3.14159 / (NLAT - 1) as f32;
        sin_lat[i] = math::sin(a); cos_lat[i] = math::cos(a); }
    for j in 0..NLON { let a = j as f32 * 6.28318 / NLON as f32;
        sin_lon[j] = math::sin(a); cos_lon[j] = math::cos(a); }

    let light = normalize(v(0.5, 0.3, -0.8));
    let mut ang = 0.0f32;

    // Preallocate transformed grid
    let mut grid: [[Vec3; NLON]; NLAT] = [[v(0.0,0.0,0.0); NLON]; NLAT];

    loop {
        while let Some(k) = keyboard::read_key() {
            if k == 0x1B { framebuffer::clear(Rgb(0,0,0)); return; }
        }
        framebuffer::clear(Rgb(5, 0, 12));

        // Transform all vertices
        for i in 0..NLAT {
            for j in 0..NLON {
                let x = sin_lat[i] * cos_lon[j];
                let y = cos_lat[i];
                let z = sin_lat[i] * sin_lon[j];
                let r = rot_y(rot_x(v(x, y, z), ang * 0.3), ang);
                grid[i][j] = r;
            }
        }

        // Render quads as two triangles, backface culled, depth sorted
        let mut faces: [(f32, usize, usize, usize, usize); 1200] = [(0.0,0,0,0,0); 1200];
        let mut nf = 0usize;

        for i in 0..NLAT - 1 { for j in 0..NLON {
            let j1 = (j + 1) % NLON;
            let (a, b, c, d) = (grid[i][j], grid[i][j1], grid[i+1][j1], grid[i+1][j]);
            let normal = tri_normal(a, b, c);
            if dot(normal, v(a.x, a.y, a.z)) >= 0.0 { continue; }
            let za = (a.z + b.z + c.z) / 3.0;
            if nf < faces.len() {
                faces[nf] = (za, i, j, j1, 0); nf += 1;
            }
            let normal2 = tri_normal(a, c, d);
            if dot(normal2, v(a.x, a.y, a.z)) >= 0.0 { continue; }
            let zb = (a.z + c.z + d.z) / 3.0;
            if nf < faces.len() {
                faces[nf] = (zb, i, j, j1, 1); nf += 1;
            }
        }}

        // Sort by Z (bubble sort)
        for _ in 0..nf { for k in 0..nf-1 {
            if faces[k].0 < faces[k+1].0 { faces.swap(k, k+1); }
        }}

        for k in 0..nf {
            let (_, i, j, j1, half) = faces[k];
            let j1 = j1 % NLON;
            let (a, b, c, d) = (grid[i][j], grid[i][j1], grid[i+1][j1], grid[i+1][j]);

            let (p0, p1, p2) = if half == 0 { (a,b,c) } else { (a,c,d) };
            let normal = tri_normal(p0, p1, p2);
            let n = normalize(normal);
            let lum = dot(n, light).max(0.1).min(1.0);

            // Latitude-based color (banding)
            let lat_norm = i as f32 / (NLAT - 1) as f32;
            let r = (0.9 - lat_norm * 0.3) * lum;
            let g = (0.3 + lat_norm * 0.5) * lum;
            let bv = (0.3 + (1.0 - (lat_norm - 0.5).abs()) * 0.5) * lum;

            let color = Rgb((r * 255.0) as u8, (g * 255.0) as u8, (bv * 255.0) as u8);

            let (p0x, p0y) = project(p0, cx, cy, 350.0, 1.8);
            let (p1x, p1y) = project(p1, cx, cy, 350.0, 1.8);
            let (p2x, p2y) = project(p2, cx, cy, 350.0, 1.8);
            fill_triangle(p0x, p0y, p1x, p1y, p2x, p2y, color);
        }

        ang += 0.03;
        delay_ms(16);
    }
}

// ═══════════════════════════════════════════════════════════════════
// WIRE DODECAHEDRON — красивый многогранник
// ═══════════════════════════════════════════════════════════════════

pub unsafe fn dodecahedron() {
    let (fw, fh) = (framebuffer::width() as f32, framebuffer::height() as f32);
    let (cx, cy) = (fw / 2.0, fh / 2.0);
    let phi = 1.6180339887f32; // golden ratio

    // 20 vertices of a dodecahedron
    let vv: [Vec3; 20] = [
        v(-1.0,-1.0,-1.0), v(-1.0,-1.0, 1.0), v(-1.0, 1.0,-1.0), v(-1.0, 1.0, 1.0),
        v( 1.0,-1.0,-1.0), v( 1.0,-1.0, 1.0), v( 1.0, 1.0,-1.0), v( 1.0, 1.0, 1.0),
        v( 0.0,-1.0/phi,-phi), v( 0.0,-1.0/phi, phi), v( 0.0, 1.0/phi,-phi), v( 0.0, 1.0/phi, phi),
        v(-phi, 0.0,-1.0/phi), v(-phi, 0.0, 1.0/phi), v( phi, 0.0,-1.0/phi), v( phi, 0.0, 1.0/phi),
        v(-1.0/phi,-phi, 0.0), v(-1.0/phi, phi, 0.0), v( 1.0/phi,-phi, 0.0), v( 1.0/phi, phi, 0.0),
    ];
    // 30 edges
    let edges: [(usize, usize); 30] = [
        (0,8),(0,12),(0,16),(1,9),(1,13),(1,17),
        (2,10),(2,12),(2,18),(3,11),(3,13),(3,19),
        (4,14),(4,16),(4,18),(5,15),(5,17),(5,19),
        (6,10),(6,14),(6,18),(7,11),(7,15),(7,19),
        (8,10),(8,14),(9,11),(9,15),(12,13),(16,17),
    ];

    let mut ang = 0.0f32;
    loop {
        while let Some(k) = keyboard::read_key() {
            if k == 0x1B { framebuffer::clear(Rgb(0,0,0)); return; }
        }
        framebuffer::clear(Rgb(5, 0, 10));

        let mut tv = [v(0.0,0.0,0.0); 20];
        for i in 0..20 {
            tv[i] = rot_y(rot_x(vv[i], ang * 0.5), ang);
        }

        for (i, &(a, b)) in edges.iter().enumerate() {
            let (p1, p2) = (tv[a], tv[b]);
            let zavg = (p1.z + p2.z) / 2.0;
            let bright = ((zavg + 2.0) / 4.0 * 200.0 + 55.0) as u8;
            let hue = i as f32 / 30.0;
            let r = math::sin(hue * 6.0).abs().min(1.0);
            let g = math::sin(hue * 6.0 + 2.0).abs().min(1.0);
            let b = math::sin(hue * 6.0 + 4.0).abs().min(1.0);
            let color = Rgb(
                (r * bright as f32) as u8,
                (g * bright as f32) as u8,
                (b * bright as f32) as u8,
            );

            let (x0, y0) = project(p1, cx, cy, 280.0, 2.5);
            let (x1, y1) = project(p2, cx, cy, 280.0, 2.5);
            draw_line(x0 as i32, y0 as i32, x1 as i32, y1 as i32, color);
        }

        ang += 0.03;
        delay_ms(20);
    }
}

// ═══════════════════════════════════════════════════════════════════
// EARTH — сфера с «материками» (GIF-текстура из кода)
// ═══════════════════════════════════════════════════════════════════

pub unsafe fn earth() {
    let (fw, fh) = (framebuffer::width() as f32, framebuffer::height() as f32);
    let (cx, cy) = (fw / 2.0, fh / 2.0);

    const NL: usize = 30; const NM: usize = 40;
    let mut sl = [0.0f32; NL]; let mut cl = [0.0f32; NL];
    let mut sm = [0.0f32; NM]; let mut cm = [0.0f32; NM];
    for i in 0..NL { let a = i as f32 * 3.14159 / (NL-1) as f32;
        sl[i]=math::sin(a); cl[i]=math::cos(a); }
    for j in 0..NM { let a = j as f32 * 6.28318 / NM as f32;
        sm[j]=math::sin(a); cm[j]=math::cos(a); }

    let light = normalize(v(0.6, 0.2, -0.8));
    let mut ang = 0.0f32;

    // Procedural earth texture
    unsafe fn is_land(lat: f32, lon: f32) -> bool {
        let n1 = math::sin(lat * 4.0) * math::cos(lon * 3.0) * 0.5 + 0.5;
        let n2 = math::sin(lat * 7.0 + 1.3) * math::cos(lon * 5.0 + 0.7) * 0.5 + 0.5;
        let n3 = math::sin((lat + lon) * 3.0) * 0.3 + 0.3;
        n1 + n2 + n3 > 0.8
    }

    let mut grid: [[Vec3; NM]; NL] = [[v(0.0,0.0,0.0); NM]; NL];

    loop {
        while let Some(k) = keyboard::read_key() {
            if k == 0x1B { framebuffer::clear(Rgb(0,0,0)); return; }
        }
        framebuffer::clear(Rgb(2, 2, 10));

        for i in 0..NL { for j in 0..NM {
            let x = sl[i] * cm[j];
            let y = cl[i];
            let z = sl[i] * sm[j];
            grid[i][j] = rot_y(v(x, y, z), ang);
        }}

        let mut faces: [(f32, usize, usize, usize, usize); 2400] = [(0.0,0,0,0,0); 2400];
        let mut nf = 0;

        for i in 0..NL-1 { for j in 0..NM {
            let j1 = (j+1)%NM;
            let (a,b,c,d) = (grid[i][j], grid[i][j1], grid[i+1][j1], grid[i+1][j]);
            let n1 = tri_normal(a,b,c);
            if dot(n1, v(a.x,a.y,a.z)) < 0.0 && nf < faces.len() {
                faces[nf] = ((a.z+b.z+c.z)/3.0, i, j, j1, 0); nf+=1;
            }
            let n2 = tri_normal(a,c,d);
            if dot(n2, v(a.x,a.y,a.z)) < 0.0 && nf < faces.len() {
                faces[nf] = ((a.z+c.z+d.z)/3.0, i, j, j1, 1); nf+=1;
            }
        }}

        for _ in 0..nf { for k in 0..nf-1 {
            if faces[k].0 < faces[k+1].0 { faces.swap(k, k+1); }
        }}

        for k in 0..nf {
            let (_, i, j, j1, half) = faces[k];
            let j1 = j1 % NM;
            let (a,b,c,d) = (grid[i][j], grid[i][j1], grid[i+1][j1], grid[i+1][j]);
            let (p0,p1,p2) = if half==0 {(a,b,c)} else {(a,c,d)};
            let n = normalize(tri_normal(p0,p1,p2));

            // Texture coordinates
            let lat = i as f32 / (NL-1) as f32 * 3.14159;
            let lon = j as f32 / NM as f32 * 6.28318 + ang;
            let land = is_land(lat, lon);

            let diff = dot(n, light).max(0.0);
            let (r, g, bv) = if land {
                (0.1 + diff * 0.5, 0.3 + diff * 0.5, 0.05 + diff * 0.2)
            } else {
                (0.05 + diff * 0.2, 0.1 + diff * 0.3, 0.3 + diff * 0.6)
            };

            let color = Rgb((r*255.0)as u8, (g*255.0)as u8, (bv*255.0)as u8);
            let (x0,y0) = project(p0,cx,cy,350.0,2.0);
            let (x1,y1) = project(p1,cx,cy,350.0,2.0);
            let (x2,y2) = project(p2,cx,cy,350.0,2.0);
            fill_triangle(x0,y0,x1,y1,x2,y2,color);
        }

        ang += 0.02;
        delay_ms(16);
    }
}

// ═══════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════

fn try_key() -> Option<u8> { unsafe { keyboard::read_key() } }

unsafe fn delay_ms(ms: u32) {
    for _ in 0..ms * 4000 { cpu::inb(0x80); }
}

// ═══════════════════════════════════════════════════════════════════
// PLASMA
// ═══════════════════════════════════════════════════════════════════

pub unsafe fn plasma() {
    let fw = framebuffer::width(); let fh = framebuffer::height();
    let mut t = 0.0f32;
    loop {
        while let Some(k) = try_key() { if k==0x1B { framebuffer::clear(Rgb(0,0,0)); return; }}
        for y in (0..fh).step_by(2) { for x in (0..fw).step_by(2) {
            let v1 = (math::sin(x as f32*0.02 + t)*0.5 + 0.5)*255.0;
            let v2 = (math::sin(y as f32*0.025 + t*1.3)*0.5 + 0.5)*255.0;
            let v3 = (math::sin((x+y) as f32*0.015 + t*0.7)*0.5 + 0.5)*255.0;
            framebuffer::fill_rect(x,y,2,2,Rgb(
                (v1*0.5+v3*0.5)as u8, (v2*0.7+v1*0.3)as u8, (v3*0.6+v2*0.4)as u8,
            ));
        }}
        t += 0.05;
    }
}

// ═══════════════════════════════════════════════════════════════════
// GAME OF LIFE
// ═══════════════════════════════════════════════════════════════════

const GOL_W: usize = 100;
const GOL_H: usize = 75;
static mut GOL_A: [u8; GOL_W * GOL_H] = [0; GOL_W * GOL_H];
static mut GOL_B: [u8; GOL_W * GOL_H] = [0; GOL_W * GOL_H];

pub unsafe fn gol() {
    let (fw, fh) = (framebuffer::width(), framebuffer::height());
    let (cw, ch) = (fw / GOL_W as u32, fh / GOL_H as u32);
    let mut seed: u32 = cpu::inb(0x40) as u32 | (cpu::inb(0x40) as u32) << 8;
    for cell in GOL_A.iter_mut() {
        seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        *cell = if (seed>>16)&1!=0 {1} else {0};
    }
    let pat: &[(usize,usize)] = &[
        (1,5),(1,6),(2,5),(2,6),(11,5),(11,6),(11,7),(12,4),(12,8),
        (13,3),(13,9),(14,3),(14,9),(15,6),(16,4),(16,8),(17,5),(17,6),(17,7),(18,6),
        (21,3),(21,4),(21,5),(22,3),(22,4),(22,5),(23,2),(23,6),
        (25,1),(25,2),(25,6),(25,7),(35,3),(35,4),(36,3),(36,4),
    ];
    for &(x,y) in pat { if x<GOL_W&&y<GOL_H { GOL_A[y*GOL_W+x]=1; }}
    let mut frame=0u32;
    loop {
        while let Some(k)=try_key() { if k==0x1B { framebuffer::clear(Rgb(0,0,0)); return; }}
        for y in 0..GOL_H { for x in 0..GOL_W {
            let p = (x as u32*cw, y as u32*ch);
            if GOL_A[y*GOL_W+x]!=0 { framebuffer::fill_rect(p.0,p.1,cw,ch,Rgb(0,180,255)); }
            else { framebuffer::fill_rect(p.0,p.1,cw,ch,Rgb(5,5,10)); }
        }}
        for y in 0..GOL_H { for x in 0..GOL_W {
            let mut n=0u8;
            for ny in -1..=1 { for nx in -1..=1 {
                if nx==0&&ny==0{continue}
                let sx=(x as i32+nx+GOL_W as i32)as usize%GOL_W;
                let sy=(y as i32+ny+GOL_H as i32)as usize%GOL_H;
                if GOL_A[sy*GOL_W+sx]!=0{n+=1}
            }}
            let idx=y*GOL_W+x;
            GOL_B[idx]=if GOL_A[idx]!=0 { if n<2||n>3{0}else{(GOL_A[idx]+1).min(255)} }
                else { if n==3{1}else{0} };
        }}
        core::ptr::swap_nonoverlapping(GOL_A.as_mut_ptr(),GOL_B.as_mut_ptr(),GOL_W*GOL_H);
        frame+=1; if frame>500{break}
        delay_ms(60);
    }
    framebuffer::clear(Rgb(0,0,0));
}

// ═══════════════════════════════════════════════════════════════════
// TUNNEL
// ═══════════════════════════════════════════════════════════════════

pub unsafe fn tunnel() {
    let (fw, fh) = (framebuffer::width() as i32, framebuffer::height() as i32);
    let (cx, cy) = (fw/2, fh/2);
    let mut t = 0.0f32;
    loop {
        while let Some(k)=try_key() { if k==0x1B { framebuffer::clear(Rgb(0,0,0)); return; }}
        for y in 0..fh { for x in (0..fw).step_by(2) {
            let dx=(x-cx)as f32; let dy=(y-cy)as f32;
            let dist=math::sqrt(dx*dx+dy*dy)/200.0;
            let ang=math::atan2(dy,dx);
            let bright=(math::sin(dist*4.0-t*2.0)*0.5+0.5)*200.0+30.0;
            let hue=math::sin(ang*3.0+dist*8.0+t)*0.5+0.5;
            let c=Rgb(
                (bright*(0.5+hue*0.5))as u8, (bright*(0.3+(1.0-hue)*0.3))as u8,
                (bright*(1.0-hue*0.5))as u8,
            );
            framebuffer::put(x as u32,y as u32,c);
            if x+1<fw { framebuffer::put((x+1)as u32,y as u32,c); }
        }}
        t += 0.04;
    }
}
