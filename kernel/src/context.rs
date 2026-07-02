//! Настоящий переключатель контекста ядра.
//!
//! В отличие от простой смены CR3, здесь сохраняются и восстанавливаются
//! callee-saved регистры, RFLAGS и указатель стека ядра каждого процесса.
//! Это и есть «переключение ветки» для Round-Robin планировщика.

use core::arch::naked_asm;

/// Сохранить контекст текущей задачи в её стек, переключиться на стек `next_rsp`
/// и адресное пространство `next_cr3` (если оно ненулевое и отличается).
///
/// ABI (System V): rdi = prev_rsp_slot, rsi = next_rsp, rdx = next_cr3.
/// `prev_rsp_slot` — куда записать новый `saved_rsp` уходящего процесса.
/// После возврата выполнение продолжается на стеке `next_rsp` так, будто
/// именно та задача когда-то вызвала `context_switch`.
#[unsafe(naked)]
pub unsafe extern "C" fn context_switch(
    prev_rsp_slot: *mut u64,
    next_rsp: u64,
    next_cr3: u64,
) {
    naked_asm!(
        // Сохраняем callee-saved регистры и флаги текущей задачи.
        "push rbx",
        "push rbp",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        "pushfq",
        // Запоминаем текущий RSP в слот уходящего процесса.
        "mov [rdi], rsp",
        // Переключаемся на стек следующей задачи.
        "mov rsp, rsi",
        // Меняем CR3 только если задан и реально другой (TLB-флаш дорогой).
        "test rdx, rdx",
        "jz 2f",
        "mov rax, cr3",
        "cmp rax, rdx",
        "je 2f",
        "mov cr3, rdx",
        "2:",
        // Восстанавливаем контекст следующей задачи и возвращаемся в её поток.
        "popfq",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbp",
        "pop rbx",
        "ret",
    );
}

/// Разметить вершину стека ядра нового процесса так, чтобы первый
/// `context_switch` в него «вернулся» в `bootstrap` (который уже уведёт
/// поток в ring 3). Возвращает стартовый `saved_rsp`.
///
/// Раскладка кадра под эпилог `context_switch` (popfq; pop r15..rbx; ret):
///   +0  RFLAGS, +8 r15, +16 r14, +24 r13, +32 r12, +40 rbp, +48 rbx, +56 ret
pub unsafe fn prepare_initial_stack(
    stack_top_raw: u64,
    bootstrap: unsafe extern "C" fn() -> !,
) -> u64 {
    let top = stack_top_raw & !0xF;
    // На входе в bootstrap RSP должен быть ≡ 8 (mod 16), как после `call`.
    let entry_rsp = top - 8;
    let saved_rsp = entry_rsp - 64;

    let frame = saved_rsp as *mut u64;
    *frame.add(0) = 0x0000_0000_0000_0002; // RFLAGS: бит1 зарезервирован=1, IF=0
    *frame.add(1) = 0; // r15
    *frame.add(2) = 0; // r14
    *frame.add(3) = 0; // r13
    *frame.add(4) = 0; // r12
    *frame.add(5) = 0; // rbp
    *frame.add(6) = 0; // rbx
    *frame.add(7) = bootstrap as u64; // адрес возврата -> bootstrap

    saved_rsp
}
