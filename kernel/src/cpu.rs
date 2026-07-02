//! Низкоуровневая инициализация CPU: GDT, TSS, MSR и вход в ring 3.
//!
//! Всё состояние статично (Zero-Alloc). Раскладка GDT подобрана строго под
//! требования инструкций `syscall`/`sysret`:
//!   STAR.SYSCALL_BASE = 0x08  -> CS=0x08 (kernel code), SS=0x10 (kernel data)
//!   STAR.SYSRET_BASE  = 0x10  -> SS=0x18|3 (user data),  CS=0x20|3 (user code)

use core::arch::asm;
use core::ptr::addr_of;

// --- Селекторы сегментов в нашей GDT ---
pub const KERNEL_CS: u16 = 0x08;
pub const KERNEL_DS: u16 = 0x10;
pub const USER_DS: u16 = 0x18 | 3; // RPL = 3
pub const USER_CS: u16 = 0x20 | 3; // RPL = 3
pub const TSS_SEL: u16 = 0x28;

// --- Номера MSR ---
pub const IA32_EFER: u32 = 0xC000_0080;
pub const IA32_STAR: u32 = 0xC000_0081;
pub const IA32_LSTAR: u32 = 0xC000_0082;
pub const IA32_FMASK: u32 = 0xC000_0084;
pub const IA32_GS_BASE: u32 = 0xC000_0101;
pub const IA32_KERNEL_GS_BASE: u32 = 0xC000_0102;

const KERNEL_STACK_SIZE: usize = 16 * 1024;

/// Per-CPU блок, адресуемый через GS после `swapgs`.
/// ВНИМАНИЕ: раскладка полей фиксирована — трамплин `syscall_entry`
/// обращается к ним по смещениям gs:[0] и gs:[8].
#[repr(C)]
pub struct PerCpu {
    pub kernel_rsp: u64,       // gs:[0]  — вершина стека ядра для syscall
    pub user_rsp_scratch: u64, // gs:[8]  — временное хранилище RSP юзера
}

pub static mut PERCPU: PerCpu = PerCpu {
    kernel_rsp: 0,
    user_rsp_scratch: 0,
};

// Поле — это просто backing-память под стек; читается по адресу, а не по значению.
#[allow(dead_code)]
#[repr(align(16))]
struct AlignedStack([u8; KERNEL_STACK_SIZE]);

static mut RSP0_STACK: AlignedStack = AlignedStack([0; KERNEL_STACK_SIZE]);
static mut IST1_STACK: AlignedStack = AlignedStack([0; KERNEL_STACK_SIZE]);

/// 64-битный TSS (104 байта). `packed`, т.к. поля u64 лежат с нечётным выравниванием.
#[repr(C, packed)]
struct Tss {
    reserved0: u32,
    rsp0: u64,
    rsp1: u64,
    rsp2: u64,
    reserved1: u64,
    ist1: u64,
    ist2: u64,
    ist3: u64,
    ist4: u64,
    ist5: u64,
    ist6: u64,
    ist7: u64,
    reserved2: u64,
    reserved3: u16,
    iomap_base: u16,
}

static mut TSS: Tss = Tss {
    reserved0: 0,
    rsp0: 0,
    rsp1: 0,
    rsp2: 0,
    reserved1: 0,
    ist1: 0,
    ist2: 0,
    ist3: 0,
    ist4: 0,
    ist5: 0,
    ist6: 0,
    ist7: 0,
    reserved2: 0,
    reserved3: 0,
    iomap_base: core::mem::size_of::<Tss>() as u16, // I/O map отключена
};

/// 7 слотов: null, kcode, kdata, udata, ucode, tss(low), tss(high).
static mut GDT: [u64; 7] = [0; 7];

#[repr(C, packed)]
struct DescriptorPtr {
    limit: u16,
    base: u64,
}

/// Собрать обычный (16-байтовый смысл, 8-байтовый слот) дескриптор сегмента.
const fn segment_descriptor(access: u8, flags: u8) -> u64 {
    // base = 0, limit = 0xFFFFF (с гранулярностью страниц).
    let limit: u64 = 0xF_FFFF;
    let mut d: u64 = 0;
    d |= limit & 0xFFFF;
    d |= ((access as u64) & 0xFF) << 40;
    d |= ((limit >> 16) & 0xF) << 48;
    d |= ((flags as u64) & 0xF) << 52;
    d
}

#[inline(always)]
fn stack_top(addr: u64) -> u64 {
    // Вершина стека (растёт вниз), выровнена на 16.
    (addr + KERNEL_STACK_SIZE as u64) & !0xF
}

/// Инициализация GDT + TSS и перезагрузка сегментных регистров.
pub unsafe fn init_gdt() {
    crate::console::boot_msg(b"[CPU] rsp0/ist1...\n");
    let rsp0_top = stack_top(addr_of!(RSP0_STACK) as u64);
    let ist1_top = stack_top(addr_of!(IST1_STACK) as u64);
    TSS.rsp0 = rsp0_top;
    TSS.ist1 = ist1_top;

    // Код/данные ring0 и ring3. access: P|DPL|S|тип; flags: G|D/B|L|AVL.
    GDT[0] = 0;
    GDT[1] = segment_descriptor(0x9A, 0xA); // kernel code: exec/read, L=1
    GDT[2] = segment_descriptor(0x92, 0xC); // kernel data: read/write, D/B=1
    GDT[3] = segment_descriptor(0xF2, 0xC); // user data,  DPL=3
    GDT[4] = segment_descriptor(0xFA, 0xA); // user code,  DPL=3, L=1

    crate::console::boot_msg(b"[CPU] GDT slots filled\n");

    // Системный дескриптор TSS занимает два слота (low + high).
    let tss_base = addr_of!(TSS) as u64;
    let tss_limit = (core::mem::size_of::<Tss>() - 1) as u64;
    let mut low: u64 = 0;
    low |= tss_limit & 0xFFFF;
    low |= (tss_base & 0xFFFF) << 16;
    low |= ((tss_base >> 16) & 0xFF) << 32;
    low |= 0x89u64 << 40; // P=1, type=9 (доступный 64-битный TSS)
    low |= ((tss_limit >> 16) & 0xF) << 48;
    low |= ((tss_base >> 24) & 0xFF) << 56;
    GDT[5] = low;
    GDT[6] = (tss_base >> 32) & 0xFFFF_FFFF;
    crate::console::boot_msg(b"[CPU] TSS descriptor built\n");

    crate::console::boot_msg(b"[CPU] LGDT...\n");
    let gdtr = DescriptorPtr {
        limit: (core::mem::size_of::<[u64; 7]>() - 1) as u16,
        base: addr_of!(GDT) as u64,
    };
    asm!("lgdt [{}]", in(reg) &gdtr, options(readonly, nostack, preserves_flags));
    crate::console::boot_msg(b"[CPU] LGDT done. Reload segment regs...\n");

    // UEFI уже работает в 64-bit long mode — можем не перезагружать CS,
    // а только установить собственные селекторы данных.
    asm!(
        "mov ds, {ds:x}",
        "mov es, {ds:x}",
        "mov ss, {ds:x}",
        ds = in(reg) KERNEL_DS,
        options(preserves_flags),
    );
    crate::console::boot_msg(b"[CPU] Segment regs reloaded. LTR...\n");

    // Загрузка task register.
    asm!("ltr {0:x}", in(reg) TSS_SEL, options(nostack, preserves_flags));
    crate::console::boot_msg(b"[CPU] LTR done.\n");
}

#[inline(always)]
pub unsafe fn wrmsr(msr: u32, value: u64) {
    let low = value as u32;
    let high = (value >> 32) as u32;
    asm!(
        "wrmsr",
        in("ecx") msr,
        in("eax") low,
        in("edx") high,
        options(nostack, preserves_flags),
    );
}

#[inline(always)]
pub unsafe fn rdmsr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;
    asm!(
        "rdmsr",
        in("ecx") msr,
        out("eax") low,
        out("edx") high,
        options(nostack, preserves_flags),
    );
    ((high as u64) << 32) | (low as u64)
}

pub fn percpu_addr() -> u64 {
    addr_of!(PERCPU) as u64
}

/// Обновить вершину стека ядра текущего процесса (читается трамплином syscall).
pub unsafe fn set_kernel_rsp(rsp: u64) {
    PERCPU.kernel_rsp = rsp;
}

#[inline(always)]
pub unsafe fn read_cr3() -> u64 {
    let value: u64;
    asm!("mov {}, cr3", out(reg) value, options(nomem, nostack, preserves_flags));
    value
}

#[inline(always)]
pub unsafe fn write_cr3(value: u64) {
    asm!("mov cr3, {}", in(reg) value, options(nostack, preserves_flags));
}

const PHYS_MASK: u64 = 0x000f_ffff_ffff_f000;
const PAGE_PRESENT: u64 = 1 << 0;
const PAGE_WRITABLE: u64 = 1 << 1;
const PAGE_USER: u64 = 1 << 2;
const PAGE_HUGE: u64 = 1 << 7;

/// Сбросить из TLB трансляцию одной виртуальной страницы.
#[inline(always)]
pub unsafe fn invlpg(virt: u64) {
    asm!("invlpg [{}]", in(reg) virt, options(nostack, preserves_flags));
}

/// Отобразить виртуальную страницу `virt` на физический фрейм `phys` в дереве
/// таблиц с корнем `pml4_phys`, создавая недостающие уровни из frame-allocator.
/// Только 4 KiB-страницы. Идентичное отображение физпамяти (phys == указатель)
/// — то же допущение, что и в остальном коде работы с таблицами.
///
/// Возвращает `false`, если не удалось выделить фрейм под промежуточную таблицу
/// или путь пересекает huge-страницу (расщепление не поддерживается).
///
/// ВНИМАНИЕ: TLB не сбрасывается — при отображении в активное адресное
/// пространство вызывающий обязан сделать `invlpg(virt)`.
pub unsafe fn map_page(
    pml4_phys: u64,
    virt: u64,
    phys: u64,
    writable: bool,
    user: bool,
) -> bool {
    let i4 = ((virt >> 39) & 0x1FF) as usize;
    let i3 = ((virt >> 30) & 0x1FF) as usize;
    let i2 = ((virt >> 21) & 0x1FF) as usize;
    let i1 = ((virt >> 12) & 0x1FF) as usize;

    let pml4 = (pml4_phys & PHYS_MASK) as *mut u64;
    let Some(pdpt) = next_table(pml4.add(i4), user) else {
        return false;
    };
    let Some(pd) = next_table(pdpt.add(i3), user) else {
        return false;
    };
    let Some(pt) = next_table(pd.add(i2), user) else {
        return false;
    };

    let mut leaf = (phys & PHYS_MASK) | PAGE_PRESENT;
    if writable {
        leaf |= PAGE_WRITABLE;
    }
    if user {
        leaf |= PAGE_USER;
    }
    *pt.add(i1) = leaf;
    invlpg(virt);
    true
}

/// Вернуть указатель на следующую таблицу по записи `entry`, создав её при
/// отсутствии. Промежуточные уровни всегда writable; бит U/S поднимается, если
/// хоть одному листу ниже нужен доступ из ring 3 (жёсткость прав задаёт лист).
unsafe fn next_table(entry: *mut u64, user: bool) -> Option<*mut u64> {
    if *entry & PAGE_PRESENT == 0 {
        let frame = crate::frame::alloc_frame()?;
        let mut flags = PAGE_PRESENT | PAGE_WRITABLE;
        if user {
            flags |= PAGE_USER;
        }
        *entry = (frame & PHYS_MASK) | flags;
    } else {
        if *entry & PAGE_HUGE != 0 {
            return None; // huge-страница на пути — не расщепляем
        }
        if user {
            *entry |= PAGE_USER;
        }
    }
    Some((*entry & PHYS_MASK) as *mut u64)
}

/// Пометить диапазон виртуальных адресов как доступный из ring 3 (бит U/S).
///
/// UEFI оставляет страницы помеченными как supervisor, поэтому без этого вход
/// в ring 3 на код/стек демо немедленно вызвал бы #PF. Предполагается
/// identity-mapping физической памяти (то же допущение, что и в IPC-копировании
/// и клонировании PML4) — физ. адрес таблиц равен виртуальному.
/// MILESTONE: при появлении frame-allocator это заменится построением приватных
/// таблиц с корректными правами на этапе создания эфемерного слоя.
pub unsafe fn make_user_accessible(virt: u64, size: u64) {
    if size == 0 {
        return;
    }
    let pml4_phys = read_cr3() & PHYS_MASK;
    let start = virt & !0xFFF;
    let end = (virt + size + 0xFFF) & !0xFFF;

    let mut page = start;
    while page < end {
        set_user_for_addr(pml4_phys, page);
        page += 0x1000;
    }

    // Сбросить TLB перезагрузкой CR3.
    write_cr3(read_cr3());
}

unsafe fn set_user_for_addr(pml4_phys: u64, va: u64) {
    let pml4 = pml4_phys as *mut u64;
    let e4 = pml4.add(((va >> 39) & 0x1FF) as usize);
    if *e4 & PAGE_PRESENT == 0 {
        return;
    }
    *e4 |= PAGE_USER;

    let pdpt = (*e4 & PHYS_MASK) as *mut u64;
    let e3 = pdpt.add(((va >> 30) & 0x1FF) as usize);
    if *e3 & PAGE_PRESENT == 0 {
        return;
    }
    *e3 |= PAGE_USER;
    if *e3 & PAGE_HUGE != 0 {
        *e3 |= PAGE_WRITABLE;
        return; // 1 GiB страница
    }

    let pd = (*e3 & PHYS_MASK) as *mut u64;
    let e2 = pd.add(((va >> 21) & 0x1FF) as usize);
    if *e2 & PAGE_PRESENT == 0 {
        return;
    }
    *e2 |= PAGE_USER;
    if *e2 & PAGE_HUGE != 0 {
        *e2 |= PAGE_WRITABLE;
        return; // 2 MiB страница
    }

    let pt = (*e2 & PHYS_MASK) as *mut u64;
    let e1 = pt.add(((va >> 12) & 0x1FF) as usize);
    if *e1 & PAGE_PRESENT == 0 {
        return;
    }
    *e1 |= PAGE_USER | PAGE_WRITABLE;
}

// ---------------------------------------------------------------------------
// Port I/O — для PS/2 клавиатуры.
// ---------------------------------------------------------------------------

/// Прочитать байт из порта. Зарезервировано (парная к `outb`).
#[allow(dead_code)]
#[inline(always)]
pub unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    asm!("in al, dx", in("dx") port, out("al") val, options(nomem, nostack, preserves_flags));
    val
}

/// Записать байт в порт.
#[inline(always)]
pub unsafe fn outb(port: u16, val: u8) {
    asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack, preserves_flags));
}

/// Переход в ring 3 на заданную точку входа с заданным пользовательским стеком.
/// Возврата нет — поток продолжается в userspace, обратно только через `syscall`/прерывание.
pub unsafe fn enter_user_mode(entry: u64, user_stack: u64) -> ! {
    // Пока выполняемся в ring0, GS_BASE временно не важен; настраиваем оба так,
    // чтобы после `swapgs` внутри трамплина GS указывал на PERCPU.
    wrmsr(IA32_GS_BASE, 0);
    wrmsr(IA32_KERNEL_GS_BASE, percpu_addr());

    // Кадр для iretq (сверху вниз): SS, RSP, RFLAGS, CS, RIP.
    asm!(
        "push {ss}",
        "push {rsp}",
        "push {rflags}",
        "push {cs}",
        "push {rip}",
        "iretq",
        ss = in(reg) USER_DS as u64,
        rsp = in(reg) user_stack,
        // IF=0: ядро работает без аппаратных прерываний (кооперативный
        // планировщик, UEFI timer заглушён `cli`). Вход в ring 3 с IF=1 снова
        // открыл бы дверь тому самому таймеру -> тройная ошибка.
        rflags = in(reg) 0x002u64, // reserved bit 1 = 1, IF=0
        cs = in(reg) USER_CS as u64,
        rip = in(reg) entry,
        options(noreturn),
    );
}
