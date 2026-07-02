//! IDT — таблица дескрипторов прерываний с обработчиками-диагностами.
//!
//! Ядро не использует аппаратные прерывания (планировщик кооперативный,
//! клавиатура опрашивается через UEFI Simple Text Input). НО без собственной
//! IDT любое исключение CPU — #PF, #GP, #DF и т.п. — немедленно превращается в
//! тройную ошибку и перезагрузку машины (см. CLAUDE.md, инвариант вехи №1).
//!
//! Здесь мы ставим свою IDT: обработчики печатают вектор, код ошибки и ключевые
//! регистры в serial + UEFI ConOut, после чего останавливают CPU (`hlt`).
//! Так «немой ребут» превращается в читаемую диагностику.
//!
//! Zero-Alloc: таблица — статический массив фиксированного размера.

use core::arch::{asm, naked_asm};
use core::mem::size_of;
use core::ptr::{addr_of, addr_of_mut};

use crate::cpu;

const IDT_ENTRIES: usize = 256;

/// Гейт-дескриптор IDT (16 байт, long mode).
#[repr(C)]
#[derive(Copy, Clone)]
struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    type_attr: u8,
    offset_mid: u16,
    offset_high: u32,
    zero: u32,
}

impl IdtEntry {
    const fn missing() -> Self {
        Self {
            offset_low: 0,
            selector: 0,
            ist: 0,
            type_attr: 0,
            offset_mid: 0,
            offset_high: 0,
            zero: 0,
        }
    }

    fn set(&mut self, handler: u64, ist: u8) {
        self.offset_low = handler as u16;
        self.selector = cpu::KERNEL_CS;
        self.ist = ist & 0x7;
        self.type_attr = 0x8E; // P=1, DPL=0, тип=0xE (64-битный interrupt gate)
        self.offset_mid = (handler >> 16) as u16;
        self.offset_high = (handler >> 32) as u32;
        self.zero = 0;
    }
}

static mut IDT: [IdtEntry; IDT_ENTRIES] = [IdtEntry::missing(); IDT_ENTRIES];

#[repr(C, packed)]
struct IdtPtr {
    limit: u16,
    base: u64,
}

// ---------------------------------------------------------------------------
// ASM-заглушки на каждый вектор исключения.
//
// Единый кадр на стеке для общего обработчика (сверху вниз):
//   [rsp+0]=vector, +8=error, +16=RIP, +24=CS, +32=RFLAGS, +40=RSP, +48=SS
// Исключения без кода ошибки пушат фиктивный 0, чтобы кадр был единообразным.
// ---------------------------------------------------------------------------

macro_rules! exc_noerr {
    ($name:ident, $vec:expr) => {
        #[unsafe(naked)]
        unsafe extern "C" fn $name() {
            naked_asm!(
                "push 0",
                concat!("push ", stringify!($vec)),
                "jmp {common}",
                common = sym common_stub,
            );
        }
    };
}

macro_rules! exc_err {
    ($name:ident, $vec:expr) => {
        #[unsafe(naked)]
        unsafe extern "C" fn $name() {
            naked_asm!(
                concat!("push ", stringify!($vec)),
                "jmp {common}",
                common = sym common_stub,
            );
        }
    };
}

// Векторы без кода ошибки.
exc_noerr!(exc0, 0);
exc_noerr!(exc1, 1);
exc_noerr!(exc2, 2);
exc_noerr!(exc3, 3);
exc_noerr!(exc4, 4);
exc_noerr!(exc5, 5);
exc_noerr!(exc6, 6);
exc_noerr!(exc7, 7);
exc_noerr!(exc9, 9);
exc_noerr!(exc16, 16);
exc_noerr!(exc18, 18);
exc_noerr!(exc19, 19);
exc_noerr!(exc20, 20);
// Векторы с кодом ошибки.
exc_err!(exc8, 8); // #DF
exc_err!(exc10, 10); // #TS
exc_err!(exc11, 11); // #NP
exc_err!(exc12, 12); // #SS
exc_err!(exc13, 13); // #GP
exc_err!(exc14, 14); // #PF
exc_err!(exc17, 17); // #AC
exc_err!(exc21, 21); // #CP

/// Общий эпилог заглушек: выравнивает стек и зовёт Rust-обработчик.
#[unsafe(naked)]
unsafe extern "C" fn common_stub() {
    naked_asm!(
        "mov rdi, rsp",   // rdi -> кадр (&vector)
        "and rsp, -16",   // выравнивание под System V ABI
        "call {handler}",
        "2:",
        "hlt",
        "jmp 2b",
        handler = sym exception_handler,
    );
}

#[no_mangle]
unsafe extern "C" fn exception_handler(frame: *const u64) -> ! {
    let vector = *frame.add(0);
    let error = *frame.add(1);
    let rip = *frame.add(2);
    let cs = *frame.add(3);
    let rflags = *frame.add(4);
    let rsp = *frame.add(5);

    let cr2: u64;
    asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack, preserves_flags));

    emit(b"\n\n*** CPU EXCEPTION -- KERNEL HALTED ***\n");
    emit(b"  vector = ");
    emit_dec(vector);
    emit(b" (");
    emit(exc_name(vector));
    emit(b")\n");
    emit(b"  error  = 0x");
    emit_hex(error);
    emit(b"\n  RIP    = 0x");
    emit_hex(rip);
    emit(b"\n  CS     = 0x");
    emit_hex(cs);
    emit(b"\n  RFLAGS = 0x");
    emit_hex(rflags);
    emit(b"\n  RSP    = 0x");
    emit_hex(rsp);
    emit(b"\n  CR2    = 0x");
    emit_hex(cr2);
    emit(b"\n");

    loop {
        asm!("hlt", options(nomem, nostack, preserves_flags));
    }
}

fn exc_name(vector: u64) -> &'static [u8] {
    match vector {
        0 => b"#DE divide error",
        1 => b"#DB debug",
        2 => b"NMI",
        3 => b"#BP breakpoint",
        4 => b"#OF overflow",
        5 => b"#BR bound range",
        6 => b"#UD invalid opcode",
        7 => b"#NM device not available",
        8 => b"#DF double fault",
        10 => b"#TS invalid TSS",
        11 => b"#NP segment not present",
        12 => b"#SS stack fault",
        13 => b"#GP general protection",
        14 => b"#PF page fault",
        16 => b"#MF x87 FP",
        17 => b"#AC alignment check",
        18 => b"#MC machine check",
        19 => b"#XM SIMD FP",
        20 => b"#VE virtualization",
        21 => b"#CP control protection",
        _ => b"reserved/unknown",
    }
}

// Диагностика идёт сразу в два канала: serial (для QEMU -serial stdio) и
// экранный терминал (глифы в GOP-фреймбуфер).
fn emit(s: &[u8]) {
    crate::console::serial_puts(s);
    crate::terminal::write(s);
}

fn emit_hex(val: u64) {
    let hex = b"0123456789abcdef";
    let mut buf = [0u8; 16];
    for i in 0..16 {
        buf[i] = hex[((val >> ((15 - i) * 4)) & 0xF) as usize];
    }
    emit(&buf);
}

fn emit_dec(val: u64) {
    if val == 0 {
        emit(b"0");
        return;
    }
    let mut buf = [0u8; 20];
    let mut i = buf.len();
    let mut v = val;
    while v > 0 {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
    }
    emit(&buf[i..]);
}

/// Заполнить IDT обработчиками исключений. Критичные (#DF/#GP/#PF) уводятся на
/// отдельный стек IST1 (настроен в `cpu::init_gdt`), чтобы диагностика работала
/// даже при повреждённом стеке ядра.
pub unsafe fn init() {
    let idt = addr_of_mut!(IDT);
    let set = |vec: usize, handler: unsafe extern "C" fn(), ist: u8| {
        (*idt)[vec].set(handler as u64, ist);
    };

    set(0, exc0, 0);
    set(1, exc1, 0);
    set(2, exc2, 0);
    set(3, exc3, 0);
    set(4, exc4, 0);
    set(5, exc5, 0);
    set(6, exc6, 0);
    set(7, exc7, 0);
    set(8, exc8, 1); // #DF -> IST1
    set(9, exc9, 0);
    set(10, exc10, 0);
    set(11, exc11, 0);
    set(12, exc12, 1); // #SS -> IST1
    set(13, exc13, 1); // #GP -> IST1
    set(14, exc14, 1); // #PF -> IST1
    set(16, exc16, 0);
    set(17, exc17, 0);
    set(18, exc18, 0);
    set(19, exc19, 0);
    set(20, exc20, 0);
    set(21, exc21, 0);
}

/// Загрузить IDT в CPU (`lidt`). Вызывать после `cpu::init_gdt`.
pub unsafe fn load() {
    let ptr = IdtPtr {
        limit: (size_of::<[IdtEntry; IDT_ENTRIES]>() - 1) as u16,
        base: addr_of!(IDT) as u64,
    };
    asm!("lidt [{}]", in(reg) &ptr, options(readonly, nostack, preserves_flags));
}
