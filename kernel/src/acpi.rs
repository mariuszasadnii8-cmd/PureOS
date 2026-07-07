use core::ptr::read_unaligned;

// ACPI 2.0 GUID: 8868E871-E4F1-11D3-BC22-0080C73C8881
const ACPI20_GUID: [u8; 16] = [
    0x71, 0xE8, 0x68, 0x88, 0xF1, 0xE4, 0xD3, 0x11,
    0xBC, 0x22, 0x00, 0x80, 0xC7, 0x3C, 0x88, 0x81,
];

const MADT_TYPE_LOCAL_APIC: u8 = 0;
const MADT_TYPE_LOCAL_X2APIC: u8 = 5;

macro_rules! rd32 {
    ($p:expr, $off:expr) => { read_unaligned(($p as *const u8).add($off) as *const u32) };
}
macro_rules! rd64 {
    ($p:expr, $off:expr) => { read_unaligned(($p as *const u8).add($off) as *const u64) };
}
macro_rules! rd8 {
    ($p:expr, $off:expr) => { read_unaligned(($p as *const u8).add($off) as *const u8) };
}

/// Найти RSDP (Root System Descriptor Pointer) ACPI 2.0+
/// через UEFI Configuration Table.
///
/// `st_addr` — физический адрес UEFI SystemTable.
/// Возвращает адрес RSDP или 0.
pub unsafe fn find_rsdp(st_addr: u64) -> u64 {
    if st_addr == 0 {
        return 0;
    }

    // Offsets from uefi-raw SystemTable repr(C):
    //   number_of_configuration_table_entries: usize at +104
    //   configuration_table: *mut ConfigurationTable at +112
    let num_entries = rd64!(st_addr, 104);
    let config_table_ptr = rd64!(st_addr, 112);

    if config_table_ptr == 0 || num_entries == 0 {
        return 0;
    }

    // ConfigurationTable: Guid(16) + vendor_table(8) = 24 bytes per entry
    for i in 0..num_entries {
        let entry = config_table_ptr + i * 24;
        let mut matched = true;
        for j in 0..16 {
            if rd8!(entry, j) != ACPI20_GUID[j] {
                matched = false;
                break;
            }
        }
        if matched {
            return rd64!(entry, 16);
        }
    }
    0
}

/// Провалидировать RSDP: сигнатура "RSD PTR " + extended checksum.
pub unsafe fn validate_rsdp(rsdp: u64) -> bool {
    if rsdp == 0 {
        return false;
    }
    // Signature "RSD PTR " in little-endian u64
    if rd64!(rsdp, 0) != 0x2052_5444_2044_5352 {
        return false;
    }
    let revision = rd8!(rsdp, 15);
    if revision < 2 {
        return false;
    }
    let length = rd32!(rsdp, 20) as usize;
    if length < 36 {
        return false;
    }
    let mut sum: u8 = 0;
    for i in 0..length {
        sum = sum.wrapping_add(rd8!(rsdp, i));
    }
    sum == 0
}

/// Найти ACPI-таблицу по сигнатуре (b"APIC") через XSDT.
pub unsafe fn find_table(rsdp: u64, sig: &[u8; 4]) -> u64 {
    if rsdp == 0 {
        return 0;
    }
    let xsdt_addr = rd64!(rsdp, 24);
    if xsdt_addr == 0 {
        return 0;
    }
    let xsdt_len = rd32!(xsdt_addr, 4) as usize;
    if xsdt_len < 36 {
        return 0;
    }
    let entry_count = (xsdt_len - 36) / 8;
    let wanted = u32::from_le_bytes(*sig);
    for i in 0..entry_count {
        let entry_addr = rd64!(xsdt_addr, 36 + i * 8);
        if entry_addr == 0 {
            continue;
        }
        if rd32!(entry_addr, 0) == wanted {
            return entry_addr;
        }
    }
    0
}

/// Распарсить MADT и заполнить массив APIC ID.
/// Возвращает количество найденных APIC ID (включая BSP — вызывающий
/// должен пропустить bsp_apic_id).
pub unsafe fn parse_madt(madt: u64, apic_ids: &mut [u32]) -> usize {
    if madt == 0 || apic_ids.is_empty() {
        return 0;
    }
    let length = rd32!(madt, 4) as usize;
    if length < 44 {
        return 0;
    }
    let mut count = 0;
    let mut offset = 44usize;
    while offset + 1 < length {
        let entry_type = rd8!(madt, offset);
        let entry_len = rd8!(madt, offset + 1) as usize;
        if entry_len < 2 {
            break;
        }
        if entry_type == MADT_TYPE_LOCAL_APIC && entry_len >= 8 && offset + 8 <= length {
            let flags = rd32!(madt, offset + 4);
            if flags & 1 != 0 {
                let apic_id = rd8!(madt, offset + 3) as u32;
                if count < apic_ids.len() {
                    apic_ids[count] = apic_id;
                    count += 1;
                }
            }
        } else if entry_type == MADT_TYPE_LOCAL_X2APIC && entry_len >= 16 && offset + 16 <= length {
            let flags = rd32!(madt, offset + 8);
            if flags & 1 != 0 {
                let apic_id = rd32!(madt, offset + 4);
                if count < apic_ids.len() {
                    apic_ids[count] = apic_id;
                    count += 1;
                }
            }
        }
        offset += entry_len;
    }
    count
}
