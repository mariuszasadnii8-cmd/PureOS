//! PureOS syscall, process table, round-robin scheduler and rendezvous IPC.
//!
//! Всё состояние ядра живёт в фиксированной статической памяти. Куча не
//! используется, IPC-сообщения никогда не буферизуются в RAM ядра.
//!
//! Инвариант адресных пространств: ядро (его код, стеки, PROCESS_TABLE и т.д.)
//! отображено одинаково во всех PML4 процессов. Поэтому переключение CR3 во
//! время `context_switch` и побайтового IPC-копирования безопасно — выполняемый
//! код и структуры ядра остаются доступны после смены таблиц страниц.

use core::arch::naked_asm;
use core::ptr::{addr_of, addr_of_mut, copy_nonoverlapping, read_volatile, write_volatile};

use crate::context;
use crate::cpu;

pub const MAX_PROCESSES: usize = 64;
pub const IPC_MESSAGE_SIZE: usize = 64;

const PAGE_SIZE: u64 = 4096;
// Виртуальное окно эфемерных слоёв. Вынесено в заведомо не занятый identity-
// отображением диапазон (16 TiB, PML4[32]), чтобы отображения слоя не
// пересекались с identity-map физпамяти, унаследованной от UEFI.
const EPHEMERAL_BASE: u64 = 0x0000_1000_0000_0000;
const EPHEMERAL_BYTES_PER_PROCESS: u64 = 16 * 1024 * 1024;

const KERNEL_STACK_BYTES: usize = 16 * 1024;
const USER_STACK_BYTES: usize = 64 * 1024;

pub(crate) const ERR_INVALID_SYSCALL: i64 = -1;
pub(crate) const ERR_INVALID_PROCESS: i64 = -2;
pub(crate) const ERR_NO_CAPACITY: i64 = -3;
pub(crate) const ERR_INVALID_POINTER: i64 = -4;
pub(crate) const ERR_OUT_OF_MEMORY: i64 = -5;
pub(crate) const ERR_UNSUPPORTED: i64 = -38;

#[derive(Copy, Clone, Eq, PartialEq)]
pub(crate) enum ProcessState {
    Empty,
    Runnable,
    BlockedOnSend { target: u64 },
    BlockedOnReceive,
    BlockedOnReply { peer: u64 },
    Exited,
}

#[derive(Copy, Clone)]
pub struct ProcessControlBlock {
    pub(crate) id: u64,
    pub(crate) state: ProcessState,
    pub(crate) page_table_base: u64,
    pub(crate) entry: u64,
    pub(crate) user_stack: u64,
    pub(crate) layer_base: u64,
    pub(crate) layer_size: u64,
    pub(crate) next_free: u64,
    pub(crate) ipc_buffer: u64,
    pub(crate) ipc_peer: u64,
    pub(crate) saved_rsp: u64,
    pub(crate) kernel_stack_top: u64,
    pub(crate) exit_code: u64,
}

impl ProcessControlBlock {
    const fn empty() -> Self {
        Self {
            id: 0,
            state: ProcessState::Empty,
            page_table_base: 0,
            entry: 0,
            user_stack: 0,
            layer_base: 0,
            layer_size: 0,
            next_free: 0,
            ipc_buffer: 0,
            ipc_peer: 0,
            saved_rsp: 0,
            kernel_stack_top: 0,
            exit_code: 0,
        }
    }

    const fn is_live(self) -> bool {
        !matches!(self.state, ProcessState::Empty | ProcessState::Exited)
    }

    fn contains_range(self, ptr: u64, len: usize) -> bool {
        let Some(end) = ptr.checked_add(len as u64) else {
            return false;
        };
        ptr >= self.layer_base && end <= self.layer_base + self.layer_size
    }
}

#[repr(C, align(4096))]
#[derive(Copy, Clone)]
struct PageTable([u64; 512]);

impl PageTable {
    const fn zero() -> Self {
        Self([0; 512])
    }
}

// Поля обёрток — просто backing-память под стеки; используются по адресу.
#[allow(dead_code)]
#[repr(align(16))]
struct KernelStack([u8; KERNEL_STACK_BYTES]);

#[allow(dead_code)]
#[repr(align(16))]
struct UserStack([u8; USER_STACK_BYTES]);

pub(crate) static mut PROCESS_TABLE: [ProcessControlBlock; MAX_PROCESSES] =
    [ProcessControlBlock::empty(); MAX_PROCESSES];
static mut PROCESS_PML4: [PageTable; MAX_PROCESSES] = [PageTable::zero(); MAX_PROCESSES];
static mut KERNEL_STACKS: [KernelStack; MAX_PROCESSES] =
    [const { KernelStack([0; KERNEL_STACK_BYTES]) }; MAX_PROCESSES];
static mut USER_STACKS: [UserStack; MAX_PROCESSES] =
    [const { UserStack([0; USER_STACK_BYTES]) }; MAX_PROCESSES];
pub(crate) static mut CURRENT_PROCESS_ID: u64 = 0;
static mut PROCESS_TABLE_READY: bool = false;

#[inline(always)]
fn kernel_stack_top(index: usize) -> u64 {
    let base = unsafe { addr_of!(KERNEL_STACKS[index]) } as u64;
    (base + KERNEL_STACK_BYTES as u64) & !0xF
}

#[inline(always)]
fn user_stack_top(index: usize) -> u64 {
    let base = unsafe { addr_of!(USER_STACKS[index]) } as u64;
    (base + USER_STACK_BYTES as u64) & !0xF
}

pub unsafe fn init_process_manager() {
    if PROCESS_TABLE_READY {
        return;
    }

    // Процесс 0 — резидентный поток ядра (idle + планировщик), ring 0.
    let cr3 = cpu::read_cr3();
    PROCESS_TABLE[0] = ProcessControlBlock {
        id: 0,
        state: ProcessState::Runnable,
        page_table_base: cr3,
        entry: 0,
        user_stack: 0,
        layer_base: EPHEMERAL_BASE,
        layer_size: EPHEMERAL_BYTES_PER_PROCESS,
        next_free: EPHEMERAL_BASE,
        ipc_buffer: 0,
        ipc_peer: 0,
        saved_rsp: 0,
        kernel_stack_top: kernel_stack_top(0),
        exit_code: 0,
    };
    CURRENT_PROCESS_ID = 0;
    PROCESS_TABLE_READY = true;
}

// ---------------------------------------------------------------------------
// Мост userspace -> ядро: настройка MSR и asm-трамплин инструкции `syscall`.
// ---------------------------------------------------------------------------

/// Включить расширения SYSCALL и привязать трамплин.
pub unsafe fn init_syscall_msrs() {
    // EFER.SCE = 1
    let efer = cpu::rdmsr(cpu::IA32_EFER);
    cpu::wrmsr(cpu::IA32_EFER, efer | 1);

    // STAR: SYSCALL грузит CS=0x08/SS=0x10, SYSRET грузит CS=0x20|3 / SS=0x18|3.
    cpu::wrmsr(cpu::IA32_STAR, (0x10u64 << 48) | (0x08u64 << 32));
    // LSTAR: точка входа ядра.
    cpu::wrmsr(cpu::IA32_LSTAR, syscall_entry as *const () as u64);
    // FMASK: при входе гасим IF, DF, TF, AC.
    cpu::wrmsr(cpu::IA32_FMASK, 0x0004_0700);

    // GS для swapgs: KERNEL_GS_BASE -> PerCpu, пользовательский GS = 0.
    cpu::wrmsr(cpu::IA32_KERNEL_GS_BASE, cpu::percpu_addr());
    cpu::wrmsr(cpu::IA32_GS_BASE, 0);

    cpu::set_kernel_rsp(PROCESS_TABLE[CURRENT_PROCESS_ID as usize].kernel_stack_top);
}

/// Трамплин инструкции `syscall` из ring 3.
///
/// На входе: rax=sys_no, rdi=arg1, rsi=arg2, rdx=arg3, rcx=user RIP,
/// r11=user RFLAGS, rsp=user stack, GS=пользовательский.
#[unsafe(naked)]
pub unsafe extern "C" fn syscall_entry() {
    naked_asm!(
        "swapgs",                  // GS -> PerCpu
        "mov gs:[8], rsp",         // user_rsp_scratch = user rsp
        "mov rsp, gs:[0]",         // переключиться на стек ядра процесса
        "push rcx",                // user RIP
        "push r11",                // user RFLAGS
        "push qword ptr gs:[8]",   // user RSP
        "sub rsp, 8",              // выравнивание стека до 16 перед call
        // System V маршалинг: handler(sys_no, arg1, arg2, arg3)
        "mov rcx, rdx",            // arg3 -> 4-й аргумент
        "mov rdx, rsi",            // arg2 -> 3-й
        "mov rsi, rdi",            // arg1 -> 2-й
        "mov rdi, rax",            // sys_no -> 1-й
        "call {handler}",
        // rax = результат syscall, сохраняем его.
        "add rsp, 8",
        "pop r10",                 // user RSP
        "pop r11",                 // user RFLAGS
        "pop rcx",                 // user RIP
        "mov rsp, r10",            // вернуть пользовательский стек
        "swapgs",
        "sysretq",
        handler = sym sys_call_handler,
    );
}

#[no_mangle]
pub unsafe extern "C" fn sys_call_handler(
    sys_no: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
) -> i64 {
    if !PROCESS_TABLE_READY {
        init_process_manager();
    }

    match sys_no {
        1 => memory_allocate(arg1, arg2),
        2 => memory_free(arg1),
        3 => create_process(arg1),
        4 => create_thread(arg1, arg2),
        5 => yield_cpu(),
        6 => exit_process(arg1),
        7 => send_ipc(arg1, arg2),
        8 => receive_ipc(arg1),
        9 => reply_ipc(arg1, arg2),
        10 => share_memory(arg1, arg2, arg3),
        11 => pci_device_access(arg1, arg2),
        12 => map_physical_memory(arg1, arg2),
        13 => create_shared_buffer(arg1, arg2),
        14 => wait_for_vblank(),
        15 => crate::elf::exec(arg1, arg2),
        // Файловые/дескрипторные syscalls (MVP — работают с терминалом)
        16 => sys_write(arg1 as u32, arg2, arg3 as usize),
        17 => sys_read(arg1 as u32, arg2, arg3 as usize),
        18 => sys_open(arg2, arg1),
        19 => sys_close(arg1 as u32),
        20 => sys_lseek(arg1 as u32, arg2 as i64, arg3 as u32),
        21 => sys_stat(arg1, arg2),
        22 => sys_dup(arg1 as u32),
        23 => sys_fcntl(arg1 as u32, arg2 as u32, arg3),
        // ====== МАГИЧЕСКИЕ SYSCALL (24-31) ======
        // Простые высокоуровневые вызовы для быстрого юзерленда
        24 => sys_print(arg2 as *const u8, arg3 as usize),
        25 => sys_println(arg2 as *const u8, arg3 as usize),
        26 => sys_input(arg1 as *mut u8, arg2 as usize),
        27 => sys_ticks(),
        28 => sys_cls(),
        29 => sys_set_cursor(arg1 as u32, arg2 as u32),
        30 => sys_color(arg1 as u32, arg2 as u32),
        31 => sys_reboot(),
        // 32: print_num(value, newline) — вывод целого; rsi!=0 добавляет '\n'.
        // Нужен скомпилированным программам (barrelc): числа в ring3 без itoa.
        32 => sys_print_num(arg1, arg2),
        // Графические syscalls (33-40)
        33 => sys_get_screen_info(arg1),
        34 => sys_draw_pixel(arg1, arg2, arg3),
        35 => sys_draw_line(arg1, arg2, arg3),
        36 => sys_draw_rect(arg1, arg2, arg3),
        37 => sys_draw_circle(arg1, arg2, arg3),
        38 => sys_draw_image(arg1, arg2, arg3),
        39 => sys_clear_screen(arg1),
        40 => sys_set_font_scale(arg1),
        _ => ERR_INVALID_SYSCALL,
    }
}

// ---------------------------------------------------------------------------
// Создание процессов.
// ---------------------------------------------------------------------------

/// Точка возврата первого `context_switch` в новый процесс: уводит поток в ring 3.
unsafe extern "C" fn task_bootstrap() -> ! {
    let pid = CURRENT_PROCESS_ID as usize;
    let entry = PROCESS_TABLE[pid].entry;
    let stack = PROCESS_TABLE[pid].user_stack;
    cpu::enter_user_mode(entry, stack);
}

pub(crate) fn find_free_slot() -> Option<usize> {
    for i in 1..MAX_PROCESSES {
        let state = unsafe { PROCESS_TABLE[i].state };
        if matches!(state, ProcessState::Empty | ProcessState::Exited) {
            return Some(i);
        }
    }
    None
}

/// Подготовить общие поля PCB нового процесса: стек ядра, заготовленный кадр
/// контекста и пользовательский стек.
unsafe fn provision_pcb(i: usize, entry: u64, page_table_base: u64, user_stack: u64) {
    let kstack_top = kernel_stack_top(i);
    let saved_rsp = context::prepare_initial_stack(kstack_top, task_bootstrap);

    let layer_base = EPHEMERAL_BASE + (i as u64 * EPHEMERAL_BYTES_PER_PROCESS);
    PROCESS_TABLE[i] = ProcessControlBlock {
        id: i as u64,
        state: ProcessState::Runnable,
        page_table_base,
        entry,
        user_stack,
        layer_base,
        layer_size: EPHEMERAL_BYTES_PER_PROCESS,
        next_free: layer_base,
        ipc_buffer: 0,
        ipc_peer: 0,
        saved_rsp,
        kernel_stack_top: kstack_top,
        exit_code: 0,
    };
}

/// Подготовить PCB для процесса, загруженного из ELF (используется elf.rs).
/// Отличается от обычного provision_pcb: код уже отображён, эфемерный слой
/// не нужен (или overlay). Сохраняем кодовый диапазон для проверки pointer'ов.
pub(crate) unsafe fn provision_pcb_elf(
    slot: usize,
    entry: u64,
    page_table_base: u64,
    user_stack: u64,
    code_base: u64,
    code_size: u64,
) {
    let kstack_top = kernel_stack_top(slot);
    let saved_rsp = context::prepare_initial_stack(kstack_top, task_bootstrap);

    PROCESS_TABLE[slot] = ProcessControlBlock {
        id: slot as u64,
        state: ProcessState::Runnable,
        page_table_base,
        entry,
        user_stack,
        layer_base: code_base,
        layer_size: code_size,
        next_free: code_base,
        ipc_buffer: 0,
        ipc_peer: 0,
        saved_rsp,
        kernel_stack_top: kstack_top,
        exit_code: 0,
    };
}

/// Запустить первый пользовательский процесс в текущем (загрузочном) адресном
/// пространстве. Код и стек помечаются доступными из ring 3.
/// MILESTONE: до появления frame-allocator процессы делят адресное пространство.
pub unsafe fn spawn_initial_user(entry: u64) -> i64 {
    crate::console::boot_msg(b"[SPAWN] find_free_slot...\n");
    let Some(i) = find_free_slot() else {
        crate::console::boot_msg(b"[SPAWN] no slot!\n");
        return ERR_NO_CAPACITY;
    };
    crate::console::boot_msg(b"[SPAWN] slot ");
    crate::console::boot_msg(&[b'0' + i as u8]);
    crate::console::boot_msg(b"\n");

    let cr3 = cpu::read_cr3();
    let stack_top = user_stack_top(i);
    crate::console::boot_msg(b"[SPAWN] provision_pcb...\n");
    provision_pcb(i, entry, cr3, stack_top);

    // Начальный процесс делит загрузочное адресное пространство (общий CR3). UEFI
    // помечает страницы как supervisor, поэтому вход в ring 3 на код/стек демо без
    // этого немедленно дал бы #PF. Явно отображаем нужные страницы в текущий PML4
    // с правами user + writable там, где нужен стек.
    // MILESTONE №3: заменить на приватные таблицы процесса.
    let code_page = entry & !0xFFF;
    let code_end = (code_page + 0x2000) & !0xFFF;
    let mut code_va = code_page;
    while code_va < code_end {
        if !cpu::map_page(cr3, code_va, code_va, false, true) {
            crate::console::boot_msg(b"[SPAWN] code map failed\n");
            return ERR_OUT_OF_MEMORY;
        }
        code_va += 0x1000;
    }

    let stack_base = addr_of!(USER_STACKS[i]) as u64;
    let stack_start = stack_base & !0xFFF;
    let stack_end = (stack_base + USER_STACK_BYTES as u64 + 0xFFF) & !0xFFF;
    let mut stack_va = stack_start;
    while stack_va < stack_end {
        if !cpu::map_page(cr3, stack_va, stack_va, true, true) {
            crate::console::boot_msg(b"[SPAWN] stack map failed\n");
            return ERR_OUT_OF_MEMORY;
        }
        stack_va += 0x1000;
    }

    crate::console::boot_msg(b"[SPAWN] OK\n");
    i as i64
}

unsafe fn create_process(entry: u64) -> i64 {
    let Some(i) = find_free_slot() else {
        return ERR_NO_CAPACITY;
    };

    clone_current_pml4_for(i);
    let stack_top = user_stack_top(i);
    provision_pcb(i, entry, process_pml4_phys(i), stack_top);
    i as i64
}

unsafe fn create_thread(entry: u64, stack: u64) -> i64 {
    let pid = create_process(entry);
    if pid < 0 {
        return pid;
    }

    let pcb = &mut PROCESS_TABLE[pid as usize];
    if stack != 0 {
        if !pcb.contains_range(stack, 1) {
            pcb.state = ProcessState::Exited;
            return ERR_INVALID_POINTER;
        }
        pcb.user_stack = stack;
    }
    pid
}

// ---------------------------------------------------------------------------
// Память эфемерного слоя (bump-allocator, без освобождения).
// ---------------------------------------------------------------------------

unsafe fn memory_allocate(size: u64, _flags: u64) -> i64 {
    if size == 0 {
        return ERR_INVALID_POINTER;
    }

    let pid = CURRENT_PROCESS_ID as usize;
    let pcb = &mut PROCESS_TABLE[pid];
    if !pcb.is_live() {
        return ERR_INVALID_PROCESS;
    }

    let aligned_size = align_up(size, PAGE_SIZE);
    let Some(end) = pcb.next_free.checked_add(aligned_size) else {
        return ERR_OUT_OF_MEMORY;
    };
    if end > pcb.layer_base + pcb.layer_size {
        return ERR_OUT_OF_MEMORY;
    }

    let allocated = pcb.next_free;
    let pml4 = pcb.page_table_base;

    // Отобразить каждую страницу диапазона на свежий физический фрейм с правами
    // ring 3 + запись. Слой — это и есть эфемерная «ветка» процесса.
    let mut va = allocated;
    while va < end {
        let Some(phys) = crate::frame::alloc_frame() else {
            return ERR_OUT_OF_MEMORY;
        };
        if !cpu::map_page(pml4, va, phys, true, true) {
            return ERR_OUT_OF_MEMORY;
        }
        // Слой отображается в адресное пространство текущего процесса, которое
        // сейчас активно (CR3 == pml4), поэтому сбрасываем его трансляцию в TLB.
        cpu::invlpg(va);
        va += PAGE_SIZE;
    }

    pcb.next_free = end;
    allocated as i64
}

unsafe fn memory_free(addr: u64) -> i64 {
    let pid = CURRENT_PROCESS_ID as usize;
    if PROCESS_TABLE[pid].contains_range(addr, 1) {
        0
    } else {
        ERR_INVALID_POINTER
    }
}

// ---------------------------------------------------------------------------
// Планировщик Round-Robin поверх настоящего переключения контекста.
// ---------------------------------------------------------------------------

static mut SCHEDULER_TICK: u64 = 0;

/// Резидентный цикл процесса 0: планирует следующий runnable процесс, иначе спит.
/// Между переключениями дёргает анимацию на фреймбуфере (доказательство жизни).
/// Сейчас роль планировщика в процессе 0 выполняет `shell::run` (shell + RR);
/// оставлено как «чистый» вариант планировщика без оболочки.
#[allow(dead_code)]
pub unsafe fn run_scheduler() -> ! {
    loop {
        let current = CURRENT_PROCESS_ID as usize;
        match next_runnable_after(current) {
            Some(next) => switch_context(current, next),
            None => core::arch::asm!("hlt", options(nomem, nostack, preserves_flags)),
        }
    }
}

/// Запустить процесс `slot` из текущего (обычно shell, процесс 0) и вернуться,
/// когда тот отдаст CPU или завершится (`exit` → `block_current` → назад к нам).
/// Используется shell'ом для запуска скомпилированной программы синхронно.
pub unsafe fn run_slot(slot: usize) {
    let current = CURRENT_PROCESS_ID as usize;
    if slot == current || slot >= MAX_PROCESSES {
        return;
    }
    if !matches!(PROCESS_TABLE[slot].state, ProcessState::Runnable) {
        return;
    }
    switch_context(current, slot);
}

pub(crate) unsafe fn switch_context(prev: usize, next: usize) {
    if prev == next {
        return;
    }
    CURRENT_PROCESS_ID = next as u64;
    cpu::set_kernel_rsp(PROCESS_TABLE[next].kernel_stack_top);

    let prev_slot = addr_of_mut!(PROCESS_TABLE[prev].saved_rsp);
    let next_rsp = PROCESS_TABLE[next].saved_rsp;
    let next_cr3 = PROCESS_TABLE[next].page_table_base;
    context::context_switch(prev_slot, next_rsp, next_cr3);
}

/// Кооперативная уступка CPU. Текущий процесс остаётся runnable.
unsafe fn yield_cpu() -> i64 {
    let current = CURRENT_PROCESS_ID as usize;
    if let Some(next) = next_runnable_after(current) {
        switch_context(current, next);
    }
    0
}

/// Заблокировать текущий процесс (его состояние уже выставлено вызывающим) и
/// отдать CPU. Возврат происходит, когда процесс снова станет runnable.
unsafe fn block_current(current: usize) {
    loop {
        if let Some(next) = next_runnable_after(current) {
            switch_context(current, next);
            return;
        }
        // Некого планировать. В кооперативной модели без таймера это означает
        // ожидание (для прод-системы здесь будет ожидание прерывания).
        core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
    }
}

unsafe fn exit_process(code: u64) -> i64 {
    let current = CURRENT_PROCESS_ID as usize;
    PROCESS_TABLE[current].state = ProcessState::Exited;
    PROCESS_TABLE[current].exit_code = code;
    // Назад этот процесс уже не вернётся — планируем кого-то ещё навсегда.
    block_current(current);
    0
}

// ---------------------------------------------------------------------------
// Синхронный IPC по принципу рандеву (без буфера в ядре).
// ---------------------------------------------------------------------------

unsafe fn send_ipc(target_proc: u64, msg_ptr: u64) -> i64 {
    let current = CURRENT_PROCESS_ID as usize;
    let target = target_proc as usize;
    if current == target || !valid_live_process(target) {
        return ERR_INVALID_PROCESS;
    }
    if !PROCESS_TABLE[current].contains_range(msg_ptr, IPC_MESSAGE_SIZE) {
        return ERR_INVALID_POINTER;
    }

    if matches!(PROCESS_TABLE[target].state, ProcessState::BlockedOnReceive) {
        // Приёмник уже ждёт — рандеву прямо сейчас.
        let dst = PROCESS_TABLE[target].ipc_buffer;
        if !PROCESS_TABLE[target].contains_range(dst, IPC_MESSAGE_SIZE) {
            return ERR_INVALID_POINTER;
        }
        copy_user_message(current, msg_ptr, target, dst, IPC_MESSAGE_SIZE);
        PROCESS_TABLE[target].ipc_peer = current as u64;
        PROCESS_TABLE[target].state = ProcessState::Runnable;
        PROCESS_TABLE[current].ipc_buffer = msg_ptr;
        PROCESS_TABLE[current].state = ProcessState::BlockedOnReply { peer: target as u64 };
    } else {
        // Приёмник не готов — ждём, пока он сделает receive.
        PROCESS_TABLE[current].ipc_buffer = msg_ptr;
        PROCESS_TABLE[current].state = ProcessState::BlockedOnSend { target: target as u64 };
    }

    block_current(current); // разблокируется в reply_ipc
    0
}

unsafe fn receive_ipc(msg_ptr: u64) -> i64 {
    let current = CURRENT_PROCESS_ID as usize;
    if !PROCESS_TABLE[current].contains_range(msg_ptr, IPC_MESSAGE_SIZE) {
        return ERR_INVALID_POINTER;
    }

    for sender in 0..MAX_PROCESSES {
        if matches!(
            PROCESS_TABLE[sender].state,
            ProcessState::BlockedOnSend { target } if target == current as u64
        ) {
            let src = PROCESS_TABLE[sender].ipc_buffer;
            if !PROCESS_TABLE[sender].contains_range(src, IPC_MESSAGE_SIZE) {
                PROCESS_TABLE[sender].state = ProcessState::Exited;
                return ERR_INVALID_POINTER;
            }

            copy_user_message(sender, src, current, msg_ptr, IPC_MESSAGE_SIZE);
            PROCESS_TABLE[current].ipc_buffer = msg_ptr;
            PROCESS_TABLE[sender].state = ProcessState::BlockedOnReply { peer: current as u64 };
            return sender as i64;
        }
    }

    // Никто не шлёт — блокируемся в ожидании отправителя.
    PROCESS_TABLE[current].ipc_buffer = msg_ptr;
    PROCESS_TABLE[current].ipc_peer = 0;
    PROCESS_TABLE[current].state = ProcessState::BlockedOnReceive;
    block_current(current);
    // Пробуждены отправителем (fast-path) — он записал свой id в ipc_peer.
    PROCESS_TABLE[current].ipc_peer as i64
}

unsafe fn reply_ipc(target_proc: u64, msg_ptr: u64) -> i64 {
    let current = CURRENT_PROCESS_ID as usize;
    let target = target_proc as usize;
    if !valid_live_process(target) {
        return ERR_INVALID_PROCESS;
    }
    if !PROCESS_TABLE[current].contains_range(msg_ptr, IPC_MESSAGE_SIZE) {
        return ERR_INVALID_POINTER;
    }

    match PROCESS_TABLE[target].state {
        ProcessState::BlockedOnReply { peer } if peer == current as u64 => {
            let dst = PROCESS_TABLE[target].ipc_buffer;
            if !PROCESS_TABLE[target].contains_range(dst, IPC_MESSAGE_SIZE) {
                return ERR_INVALID_POINTER;
            }
            copy_user_message(current, msg_ptr, target, dst, IPC_MESSAGE_SIZE);
            PROCESS_TABLE[target].ipc_peer = current as u64;
            PROCESS_TABLE[target].ipc_buffer = 0;
            PROCESS_TABLE[target].state = ProcessState::Runnable;
            // Отправитель снова runnable; round-robin отдаст ему CPU.
            0
        }
        _ => ERR_INVALID_PROCESS,
    }
}

// ---------------------------------------------------------------------------
// Системные вызовы, ожидающие frame-allocator / драйверов (следующие вехи).
// ---------------------------------------------------------------------------

unsafe fn share_memory(_target_proc: u64, _addr: u64, _size: u64) -> i64 {
    ERR_UNSUPPORTED
}

unsafe fn pci_device_access(_bus_slot_func: u64, _offset: u64) -> i64 {
    ERR_UNSUPPORTED
}

unsafe fn map_physical_memory(_phys_addr: u64, _size: u64) -> i64 {
    ERR_UNSUPPORTED
}

unsafe fn create_shared_buffer(_size: u64, _flags: u64) -> i64 {
    ERR_UNSUPPORTED
}

unsafe fn wait_for_vblank() -> i64 {
    ERR_UNSUPPORTED
}

// ===================================================================
// МАГИЧЕСКИЕ SYSCALL (24-31) — высокоуровневые, простые в использовании
// ===================================================================

/// 24: print(buf, len) — напечатать строку в терминал
unsafe fn sys_print(buf: *const u8, len: usize) -> i64 {
    if buf.is_null() || len == 0 { return 0; }
    let pid = CURRENT_PROCESS_ID as usize;
    if !PROCESS_TABLE[pid].contains_range(buf as u64, len) {
        return ERR_INVALID_POINTER;
    }
    for i in 0..len {
        let ch = core::ptr::read_volatile(buf.add(i));
        crate::terminal::putchar(ch);
    }
    len as i64
}

/// 25: println(buf, len) — напечатать строку + newline
unsafe fn sys_println(buf: *const u8, len: usize) -> i64 {
    let n = sys_print(buf, len);
    crate::terminal::putchar(b'\n');
    n
}

/// 32: print_num(value, newline) — напечатать целое; newline!=0 → ещё '\n'.
unsafe fn sys_print_num(value: u64, newline: u64) -> i64 {
    crate::terminal::write_num(value);
    if newline != 0 {
        crate::terminal::putchar(b'\n');
    }
    0
}

/// 26: input(buf, maxlen) — прочитать строку с клавиатуры (блокирующая)
unsafe fn sys_input(buf: *mut u8, maxlen: usize) -> i64 {
    if buf.is_null() || maxlen == 0 { return 0; }
    let pid = CURRENT_PROCESS_ID as usize;
    if !PROCESS_TABLE[pid].contains_range(buf as u64, maxlen) {
        return ERR_INVALID_POINTER;
    }
    let mut pos: usize = 0;
    loop {
        // Переключаем контекст, пока ждём клавишу
        let current = CURRENT_PROCESS_ID as usize;
        if let Some(next) = next_runnable_after(current) {
            switch_context(current, next);
        }
        // Пробуем читать клавиатуру
        if let Some(ch) = crate::keyboard::read_key() {
            match ch {
                b'\n' | b'\r' => {
                    core::ptr::write_volatile(buf.add(pos), 0);
                    crate::terminal::putchar(b'\n');
                    return pos as i64;
                }
                0x7F | 0x08 => {
                    if pos > 0 {
                        pos -= 1;
                        crate::terminal::putchar(0x7F);
                    }
                }
                _ if ch >= 0x20 && ch < 0x7F => {
                    if pos < maxlen - 1 {
                        core::ptr::write_volatile(buf.add(pos), ch);
                        pos += 1;
                        crate::terminal::putchar(ch);
                    }
                }
                _ => {}
            }
        }
    }
}

/// 27: ticks() — возвращает счётчик тиков планировщика
unsafe fn sys_ticks() -> i64 {
    SCHEDULER_TICK as i64
}

/// 28: cls() — очистить экран терминала
unsafe fn sys_cls() -> i64 {
    crate::terminal::clear();
    0
}

/// 29: set_cursor(row, col) — установить позицию курсора
unsafe fn sys_set_cursor(_row: u32, _col: u32) -> i64 {
    0
}

/// 30: color(fg, bg) — установить цвета терминала (заглушка-демо)
unsafe fn sys_color(fg: u32, _bg: u32) -> i64 {
    fg as i64
}

/// 31: reboot() — перезагрузка через UEFI Runtime Services
unsafe fn sys_reboot() -> i64 {
    crate::uefi::reset_system()
}

// ---------------------------------------------------------------------------
// Файловые syscalls (MVP — Terminal I/O).
// ---------------------------------------------------------------------------

// Простая таблица дескрипторов процесса.
const MAX_FDS: usize = 16;

/// Записать данные в дескриптор.
/// fd=1 (stdout), fd=2 (stderr): пишет в терминал.
unsafe fn sys_write(fd: u32, buf: u64, len: usize) -> i64 {
    if fd != 1 && fd != 2 {
        return ERR_UNSUPPORTED;
    }
    if len == 0 {
        return 0;
    }
    // Проверить, что buf в адресном пространстве процесса
    let pid = CURRENT_PROCESS_ID as usize;
    if !PROCESS_TABLE[pid].contains_range(buf, len) {
        return ERR_INVALID_POINTER;
    }
    // Побайтовый вывод в терминал через volatile
    for i in 0..len {
        let ch = core::ptr::read_volatile((buf + i as u64) as *const u8);
        crate::terminal::putchar(ch);
    }
    len as i64
}

/// Прочитать данные из дескриптора.
/// fd=0 (stdin): читает из буфера клавиатуры (неблокирующий).
unsafe fn sys_read(fd: u32, buf: u64, len: usize) -> i64 {
    if fd != 0 {
        return ERR_UNSUPPORTED;
    }
    if len == 0 {
        return 0;
    }
    let pid = CURRENT_PROCESS_ID as usize;
    if !PROCESS_TABLE[pid].contains_range(buf, len) {
        return ERR_INVALID_POINTER;
    }
    // Неблокирующее чтение клавиатуры
    let mut count: i64 = 0;
    while count < len as i64 {
        if let Some(ch) = crate::keyboard::read_key() {
            core::ptr::write_volatile((buf + count as u64) as *mut u8, ch);
            count += 1;
        } else {
            break;
        }
    }
    // Если ни одного байта, возвращаем -EWOULDBLOCK (-11)
    if count == 0 {
        -11
    } else {
        count
    }
}

/// Открыть псевдо-устройство по пути.
/// Пока поддерживает только "/dev/console" (возвращает FD 1).
unsafe fn sys_open(_path: u64, _flags: u64) -> i64 {
    // В MVP — всегда возвращаем fd=1 (консоль)
    // При полной реализации нужно копировать строку из userspace
    1
}

/// Закрыть дескриптор.
unsafe fn sys_close(_fd: u32) -> i64 {
    0 // no-op в MVP
}

/// Переместить указатель в файле.
unsafe fn sys_lseek(_fd: u32, _offset: i64, _whence: u32) -> i64 {
    ERR_UNSUPPORTED // нет файловой системы
}

/// Получить информацию о файле.
unsafe fn sys_stat(_path: u64, _stat_buf: u64) -> i64 {
    ERR_UNSUPPORTED
}

/// Скопировать дескриптор.
unsafe fn sys_dup(fd: u32) -> i64 {
    if fd > MAX_FDS as u32 {
        return ERR_UNSUPPORTED;
    }
    fd as i64 // MVP: возвращаем тот же дескриптор
}

/// Управление дескриптором.
unsafe fn sys_fcntl(_fd: u32, _cmd: u32, _arg: u64) -> i64 {
    ERR_UNSUPPORTED
}

// ---------------------------------------------------------------------------
// Графические syscalls (33-40)
// ---------------------------------------------------------------------------

/// 33: get_screen_info(buf) — получить информацию о экране
/// buf указывает на структуру ScreenInfo в userspace
unsafe fn sys_get_screen_info(buf: u64) -> i64 {
    let pid = CURRENT_PROCESS_ID as usize;
    if !PROCESS_TABLE[pid].contains_range(buf, 16) {
        return ERR_INVALID_POINTER;
    }
    
    let info = crate::graphics::get_screen_info();
    core::ptr::write_volatile(buf as *mut u32, info.width);
    core::ptr::write_volatile((buf + 4) as *mut u32, info.height);
    core::ptr::write_volatile((buf + 8) as *mut u32, info.stride);
    core::ptr::write_volatile((buf + 12) as *mut u32, info.format);
    
    0
}

/// 34: draw_pixel(x, y, color) — нарисовать пиксель
/// color упакован как 0x00RRGGBB
unsafe fn sys_draw_pixel(x: u64, y: u64, color: u64) -> i64 {
    let r = ((color >> 16) & 0xFF) as u8;
    let g = ((color >> 8) & 0xFF) as u8;
    let b = (color & 0xFF) as u8;
    crate::graphics::draw_pixel(x as u32, y as u32, r, g, b);
    0
}

/// 35: draw_line(x1, y1, x2, y2, color) — нарисовать линию
/// args упакованы: arg1=(x1<<16)|y1, arg2=(x2<<16)|y2, arg3=color
unsafe fn sys_draw_line(arg1: u64, arg2: u64, color: u64) -> i64 {
    let x1 = (arg1 >> 16) as u32;
    let y1 = (arg1 & 0xFFFF) as u32;
    let x2 = (arg2 >> 16) as u32;
    let y2 = (arg2 & 0xFFFF) as u32;
    
    let r = ((color >> 16) & 0xFF) as u8;
    let g = ((color >> 8) & 0xFF) as u8;
    let b = (color & 0xFF) as u8;
    
    crate::graphics::draw_line(x1, y1, x2, y2, r, g, b);
    0
}

/// 36: draw_rect(x, y, w, h, color, fill) — нарисовать прямоугольник
/// args: arg1=(x<<16)|y, arg2=(w<<16)|h, arg3=(fill<<32)|color
unsafe fn sys_draw_rect(arg1: u64, arg2: u64, arg3: u64) -> i64 {
    let x = (arg1 >> 16) as u32;
    let y = (arg1 & 0xFFFF) as u32;
    let w = (arg2 >> 16) as u32;
    let h = (arg2 & 0xFFFF) as u32;
    let fill = (arg3 >> 32) != 0;
    let color = arg3 & 0xFFFFFFFF;
    
    let r = ((color >> 16) & 0xFF) as u8;
    let g = ((color >> 8) & 0xFF) as u8;
    let b = (color & 0xFF) as u8;
    
    crate::graphics::draw_rect(x, y, w, h, r, g, b, fill);
    0
}

/// 37: draw_circle(x, y, radius, color, fill) — нарисовать круг
/// args: arg1=(x<<16)|y, arg2=(fill<<32)|(radius<<16)|color
unsafe fn sys_draw_circle(arg1: u64, arg2: u64, _arg3: u64) -> i64 {
    let x = (arg1 >> 16) as u32;
    let y = (arg1 & 0xFFFF) as u32;
    let fill = (arg2 >> 32) != 0;
    let radius = ((arg2 >> 16) & 0xFFFF) as u32;
    let color = arg2 & 0xFFFF;
    
    let r = ((color >> 8) & 0xF) as u8;
    let g = (color & 0xF) as u8;
    let b = 0;
    
    crate::graphics::draw_circle(x, y, radius, r, g, b, fill);
    0
}

/// 38: draw_image(x, y, data, width, height) — нарисовать изображение
/// args: arg1=(x<<16)|y, arg2=data, arg3=(width<<16)|height
unsafe fn sys_draw_image(arg1: u64, data: u64, arg3: u64) -> i64 {
    let x = (arg1 >> 16) as u32;
    let y = (arg1 & 0xFFFF) as u32;
    let width = (arg3 >> 16) as u32;
    let height = arg3 as u32;
    
    let pid = CURRENT_PROCESS_ID as usize;
    let size = (width * height * 3) as usize;
    if !PROCESS_TABLE[pid].contains_range(data, size) {
        return ERR_INVALID_POINTER;
    }
    
    let slice = core::slice::from_raw_parts(data as *const u8, size);
    crate::graphics::draw_image(x, y, slice, width, height);
    0
}

/// 39: clear_screen(color) — очистить экран
/// color упакован как 0x00RRGGBB
unsafe fn sys_clear_screen(color: u64) -> i64 {
    let r = ((color >> 16) & 0xFF) as u8;
    let g = ((color >> 8) & 0xFF) as u8;
    let b = (color & 0xFF) as u8;
    crate::graphics::clear_screen(r, g, b);
    0
}

/// 40: set_font_scale(scale) — установить масштаб шрифта
unsafe fn sys_set_font_scale(scale: u64) -> i64 {
    // TODO: реализовать сохранение масштаба
    0
}

// ---------------------------------------------------------------------------
// Вспомогательные функции.
// ---------------------------------------------------------------------------

fn align_up(value: u64, alignment: u64) -> u64 {
    (value + alignment - 1) & !(alignment - 1)
}

unsafe fn valid_live_process(pid: usize) -> bool {
    pid < MAX_PROCESSES && PROCESS_TABLE[pid].is_live()
}

pub(crate) unsafe fn next_runnable_after(current: usize) -> Option<usize> {
    for step in 1..=MAX_PROCESSES {
        let candidate = (current + step) % MAX_PROCESSES;
        if matches!(PROCESS_TABLE[candidate].state, ProcessState::Runnable) {
            return Some(candidate);
        }
    }
    None
}

pub(crate) unsafe fn clone_current_pml4_for(pid: usize) {
    let src = (cpu::read_cr3() & !0xfff) as *const u64;
    let dst = addr_of_mut!(PROCESS_PML4[pid].0) as *mut u64;
    copy_nonoverlapping(src, dst, 512);
}

pub(crate) unsafe fn process_pml4_phys(pid: usize) -> u64 {
    addr_of!(PROCESS_PML4[pid]) as u64
}

/// Прямое копирование IPC-сообщения из адресного пространства отправителя в
/// адресное пространство получателя, без промежуточного буфера ядра.
/// Корректно при общем отображении ядра во всех PML4 (см. инвариант сверху).
unsafe fn copy_user_message(
    src_pid: usize,
    src_ptr: u64,
    dst_pid: usize,
    dst_ptr: u64,
    len: usize,
) {
    let restore_pid = CURRENT_PROCESS_ID as usize;
    let restore_cr3 = PROCESS_TABLE[restore_pid].page_table_base;

    for offset in 0..len {
        cpu::write_cr3(PROCESS_TABLE[src_pid].page_table_base);
        let byte = read_volatile((src_ptr + offset as u64) as *const u8);

        cpu::write_cr3(PROCESS_TABLE[dst_pid].page_table_base);
        write_volatile((dst_ptr + offset as u64) as *mut u8, byte);
    }

    cpu::write_cr3(restore_cr3);
}
