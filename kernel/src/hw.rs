//! Аппаратная детекция PureOS — «всю инфу об оборудовании берём прямо в железе».
//!
//! Источники, безопасные при работающем UEFI (без вызова firmware после подмены
//! GDT/IDT):
//!   - CPUID — vendor, brand string, семейство/модель, флаги фич, число потоков;
//!   - PCI configuration space через порты 0xCF8/0xCFC (legacy mechanism #1);
//!   - объём RAM — из пула фреймов, переданного загрузчиком.
//!
//! Zero-Alloc: результат скана PCI кэшируется в статический массив фиксированной
//! длины на этапе `init`, дальше только читается.

use crate::cpu::{inb, outb};

// ---------------------------------------------------------------------------
// Заморозка сведений
// ---------------------------------------------------------------------------

static mut RAM_BASE: u64 = 0;
static mut RAM_SIZE: u64 = 0;
static mut READY: bool = false;

const MAX_PCI: usize = 64;

#[derive(Copy, Clone)]
pub struct PciDev {
    pub bus: u8,
    pub slot: u8,
    pub func: u8,
    pub vendor: u16,
    pub device: u16,
    pub class: u8,
    pub subclass: u8,
    pub prog_if: u8,
}

impl PciDev {
    const fn empty() -> Self {
        Self {
            bus: 0, slot: 0, func: 0,
            vendor: 0, device: 0,
            class: 0, subclass: 0, prog_if: 0,
        }
    }
}

static mut PCI_DEVS: [PciDev; MAX_PCI] = [PciDev::empty(); MAX_PCI];
static mut PCI_COUNT: usize = 0;

pub unsafe fn init(ram_base: u64, ram_size: u64) {
    if READY {
        return;
    }
    RAM_BASE = ram_base;
    RAM_SIZE = ram_size;
    scan_pci();
    READY = true;
}

pub fn ram_size() -> u64 {
    unsafe { RAM_SIZE }
}

pub fn ram_base() -> u64 {
    unsafe { RAM_BASE }
}

// ---------------------------------------------------------------------------
// CPUID
// ---------------------------------------------------------------------------

#[inline(always)]
unsafe fn cpuid(leaf: u32, subleaf: u32) -> (u32, u32, u32, u32) {
    let (a, b, c, d);
    // RBX сохраняем вручную: LLVM резервирует его.
    core::arch::asm!(
        "mov {tmp:r}, rbx",
        "cpuid",
        "xchg {tmp:r}, rbx",
        tmp = out(reg) b,
        inout("eax") leaf => a,
        inout("ecx") subleaf => c,
        out("edx") d,
        options(nostack, preserves_flags),
    );
    (a, b, c, d)
}

/// Записать 12-байтовый vendor id ("GenuineIntel"/"AuthenticAMD") в out.
pub fn cpu_vendor(out: &mut [u8; 12]) {
    unsafe {
        let (_, ebx, ecx, edx) = cpuid(0, 0);
        out[0..4].copy_from_slice(&ebx.to_le_bytes());
        out[4..8].copy_from_slice(&edx.to_le_bytes());
        out[8..12].copy_from_slice(&ecx.to_le_bytes());
    }
}

/// Записать brand string процессора (до 48 байт) в out, вернуть длину.
pub fn cpu_brand(out: &mut [u8; 48]) -> usize {
    unsafe {
        let (max_ext, _, _, _) = cpuid(0x8000_0000, 0);
        if max_ext < 0x8000_0004 {
            let s = b"Unknown CPU";
            out[..s.len()].copy_from_slice(s);
            return s.len();
        }
        for (i, leaf) in [0x8000_0002u32, 0x8000_0003, 0x8000_0004].iter().enumerate() {
            let (a, b, c, d) = cpuid(*leaf, 0);
            let base = i * 16;
            out[base..base + 4].copy_from_slice(&a.to_le_bytes());
            out[base + 4..base + 8].copy_from_slice(&b.to_le_bytes());
            out[base + 8..base + 12].copy_from_slice(&c.to_le_bytes());
            out[base + 12..base + 16].copy_from_slice(&d.to_le_bytes());
        }
        // Обрезать по завершающему нулю.
        let mut len = 48;
        while len > 0 && (out[len - 1] == 0 || out[len - 1] == b' ') {
            len -= 1;
        }
        len
    }
}

/// Число логических процессоров.
///
/// Сначала пытается CPUID leaf 0Bh (Extended Topology Enumeration) —
/// он точен на современном железе (Kaby Lake+). Если не поддерживается,
/// fallback на CPUID.1:EBX[23:16].
pub fn cpu_threads() -> u32 {
    unsafe {
        // CPUID leaf 0Bh: Extended Topology Enumeration
        let (max_leaf, _, _, _) = cpuid(0, 0);
        if max_leaf >= 0x0B {
            let mut max_threads = 0u32;
            for subleaf in 0..4u32 {
                let (_, ebx, ecx, _) = cpuid(0x0B, subleaf);
                let level_type = (ecx >> 8) & 0xFF; // ECX[15:8] = SMT(1)/Core(2)/...
                let _level_num = ecx & 0xFF;         // ECX[7:0]
                if level_type == 0 {
                    break;
                }
                let n = ebx & 0xFFFF; // EBX[15:0]
                if n > max_threads {
                    max_threads = n;
                }
            }
            if max_threads > 0 {
                return max_threads;
            }
        }

        // Fallback: CPUID.1:EBX[23:16]
        let (_, ebx, _, _) = cpuid(1, 0);
        let n = (ebx >> 16) & 0xFF;
        if n == 0 { 1 } else { n }
    }
}

/// Собрать битовую сводку основных фич (CPUID.1 ECX/EDX).
pub struct CpuFeatures {
    pub sse: bool,
    pub sse2: bool,
    pub avx: bool,
    pub aes: bool,
    pub rdrand: bool,
    pub apic: bool,
    pub fpu: bool,
    pub tsc: bool,
}

pub fn cpu_features() -> CpuFeatures {
    unsafe {
        let (_, _, ecx, edx) = cpuid(1, 0);
        CpuFeatures {
            fpu: edx & (1 << 0) != 0,
            tsc: edx & (1 << 4) != 0,
            apic: edx & (1 << 9) != 0,
            sse: edx & (1 << 25) != 0,
            sse2: edx & (1 << 26) != 0,
            avx: ecx & (1 << 28) != 0,
            aes: ecx & (1 << 25) != 0,
            rdrand: ecx & (1 << 30) != 0,
        }
    }
}

/// Доступен ли RDSEED (CPUID.7:EBX[18]).
pub fn has_rdseed() -> bool {
    unsafe {
        let (_, ebx, _, _) = cpuid(7, 0);
        ebx & (1 << 18) != 0
    }
}

// ---------------------------------------------------------------------------
// PCI configuration space (mechanism #1, порты 0xCF8/0xCFC)
// ---------------------------------------------------------------------------

const PCI_CONFIG_ADDRESS: u16 = 0xCF8;
const PCI_CONFIG_DATA: u16 = 0xCFC;

unsafe fn pci_read32(bus: u8, slot: u8, func: u8, off: u8) -> u32 {
    let addr: u32 = 0x8000_0000
        | ((bus as u32) << 16)
        | ((slot as u32) << 11)
        | ((func as u32) << 8)
        | ((off as u32) & 0xFC);
    // 32-битный вывод в 0xCF8.
    outl(PCI_CONFIG_ADDRESS, addr);
    inl(PCI_CONFIG_DATA)
}

#[inline(always)]
unsafe fn outl(port: u16, val: u32) {
    core::arch::asm!("out dx, eax", in("dx") port, in("eax") val, options(nomem, nostack, preserves_flags));
}

#[inline(always)]
unsafe fn inl(port: u16) -> u32 {
    let val: u32;
    core::arch::asm!("in eax, dx", in("dx") port, out("eax") val, options(nomem, nostack, preserves_flags));
    val
}

unsafe fn scan_pci() {
    PCI_COUNT = 0;
    // Скан bus 0..=255 замедляет старт; ограничимся bus 0..8 (QEMU q35 и типичные хосты).
    for bus in 0u8..8 {
        for slot in 0u8..32 {
            let vendor_device = pci_read32(bus, slot, 0, 0x00);
            let vendor = (vendor_device & 0xFFFF) as u16;
            if vendor == 0xFFFF {
                continue; // слот пуст
            }
            // Multi-function: проверить бит 7 header type.
            let header = (pci_read32(bus, slot, 0, 0x0C) >> 16) & 0xFF;
            let funcs = if header & 0x80 != 0 { 8 } else { 1 };
            for func in 0u8..funcs {
                let vd = pci_read32(bus, slot, func, 0x00);
                let v = (vd & 0xFFFF) as u16;
                if v == 0xFFFF {
                    continue;
                }
                let dev = ((vd >> 16) & 0xFFFF) as u16;
                let class_reg = pci_read32(bus, slot, func, 0x08);
                if PCI_COUNT < MAX_PCI {
                    PCI_DEVS[PCI_COUNT] = PciDev {
                        bus, slot, func,
                        vendor: v,
                        device: dev,
                        class: ((class_reg >> 24) & 0xFF) as u8,
                        subclass: ((class_reg >> 16) & 0xFF) as u8,
                        prog_if: ((class_reg >> 8) & 0xFF) as u8,
                    };
                    PCI_COUNT += 1;
                }
            }
        }
    }
}

pub fn pci_devices() -> &'static [PciDev] {
    unsafe { core::slice::from_raw_parts(core::ptr::addr_of!(PCI_DEVS[0]), PCI_COUNT) }
}

/// Человекочитаемое имя класса PCI-устройства.
pub fn pci_class_name(class: u8, subclass: u8) -> &'static [u8] {
    match class {
        0x00 => b"Unclassified",
        0x01 => match subclass {
            0x06 => b"SATA controller",
            0x08 => b"NVMe controller",
            _ => b"Mass storage",
        },
        0x02 => b"Network controller",
        0x03 => b"Display controller",
        0x04 => b"Multimedia",
        0x06 => match subclass {
            0x00 => b"Host bridge",
            0x01 => b"ISA bridge",
            0x04 => b"PCI bridge",
            _ => b"Bridge",
        },
        0x0C => match subclass {
            0x03 => b"USB controller",
            _ => b"Serial bus",
        },
        _ => b"Device",
    }
}

// Suppress unused warning for inb/outb import in some builds.
#[allow(dead_code)]
unsafe fn _touch_ports() {
    let _ = inb(0x80);
    outb(0x80, 0);
}
