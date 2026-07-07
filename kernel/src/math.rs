//! Математические функции через SSE + x87 FPU.
//! Требуются, т.к. f32::sin() и т.п. недоступны в `no_std` без libm.

/// sin(x) через x87 FSIN.
#[inline(always)]
pub unsafe fn sin(x: f32) -> f32 {
    let mut r: f32 = 0.0;
    core::arch::asm!(
        "fld dword ptr [{0}]",
        "fsin",
        "fstp dword ptr [{1}]",
        in(reg) &x,
        in(reg) &mut r,
        options(nostack, preserves_flags),
    );
    r
}

/// cos(x) через x87 FCOS.
#[inline(always)]
pub unsafe fn cos(x: f32) -> f32 {
    let mut r: f32 = 0.0;
    core::arch::asm!(
        "fld dword ptr [{0}]",
        "fcos",
        "fstp dword ptr [{1}]",
        in(reg) &x,
        in(reg) &mut r,
        options(nostack, preserves_flags),
    );
    r
}

/// sqrt(x) через x87 FSQRT.
#[inline(always)]
pub unsafe fn sqrt(x: f32) -> f32 {
    let mut r: f32 = 0.0;
    core::arch::asm!(
        "fld dword ptr [{0}]",
        "fsqrt",
        "fstp dword ptr [{1}]",
        in(reg) &x,
        in(reg) &mut r,
        options(nostack, preserves_flags),
    );
    r
}

/// atan2(y, x) через x87 FPATAN.
pub unsafe fn atan2(y: f32, x: f32) -> f32 {
    let mut r: f32 = 0.0;
    core::arch::asm!(
        "fld dword ptr [{0}]",
        "fld dword ptr [{1}]",
        "fpatan",
        "fstp dword ptr [{2}]",
        in(reg) &x,
        in(reg) &y,
        in(reg) &mut r,
        options(nostack, preserves_flags),
    );
    r
}

pub const PI: f32 = 3.14159265358979323846;
