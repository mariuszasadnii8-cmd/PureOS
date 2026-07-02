#ifndef SYSCALLS_H
#define SYSCALLS_H

// Системные вызовы PureOS
#define SYS_MEMORY_ALLOCATE    1
#define SYS_MEMORY_FREE        2
#define SYS_CREATE_PROCESS     3
#define SYS_CREATE_THREAD      4
#define SYS_YIELD_CPU          5
#define SYS_EXIT_PROCESS       6
#define SYS_SEND_IPC           7
#define SYS_RECEIVE_IPC        8
#define SYS_REPLY_IPC          9
#define SYS_SHARE_MEMORY       10
#define SYS_PCI_DEVICE_ACCESS  11
#define SYS_MAP_PHYSICAL_MEM   12
#define SYS_CREATE_SHARED_BUF  13
#define SYS_WAIT_FOR_VBLANK    14
#define SYS_EXEC_ELF           15

// 🌟 Магические syscall (24-31) — простые, для быстрого старта
#define SYS_PRINT              24
#define SYS_PRINTLN            25
#define SYS_INPUT              26
#define SYS_TICKS              27
#define SYS_CLS                28
#define SYS_SET_CURSOR         29
#define SYS_COLOR              30
#define SYS_REBOOT             31

static inline long long __pureos_syscall(long long sys_no, long long arg1, long long arg2, long long arg3) {
    long long result;
    __asm__ volatile (
        "syscall"
        : "=a" (result)
        : "a" (sys_no), "D" (arg1), "S" (arg2), "d" (arg3)
        : "rcx", "r11", "memory"
    );
    return result;
}

// --- Базовые ---
static inline long long memory_allocate(long long size, long long flags) {
    return __pureos_syscall(SYS_MEMORY_ALLOCATE, size, flags, 0);
}
static inline long long memory_free(long long addr) {
    return __pureos_syscall(SYS_MEMORY_FREE, addr, 0, 0);
}
static inline long long create_process(long long elf_image) {
    return __pureos_syscall(SYS_CREATE_PROCESS, elf_image, 0, 0);
}
static inline long long create_thread(long long entry, long long stack) {
    return __pureos_syscall(SYS_CREATE_THREAD, entry, stack, 0);
}
static inline long long yield_cpu(void) {
    return __pureos_syscall(SYS_YIELD_CPU, 0, 0, 0);
}
static inline long long exit_process(long long code) {
    return __pureos_syscall(SYS_EXIT_PROCESS, code, 0, 0);
}
static inline long long send_ipc(long long target_proc, const void* msg_ptr) {
    return __pureos_syscall(SYS_SEND_IPC, target_proc, (long long)msg_ptr, 0);
}
static inline long long receive_ipc(void* msg_ptr) {
    return __pureos_syscall(SYS_RECEIVE_IPC, (long long)msg_ptr, 0, 0);
}
static inline long long reply_ipc(long long target_proc, const void* msg_ptr) {
    return __pureos_syscall(SYS_REPLY_IPC, target_proc, (long long)msg_ptr, 0);
}
static inline long long exec_elf(const void* elf_data, long long size) {
    return __pureos_syscall(SYS_EXEC_ELF, (long long)elf_data, size, 0);
}

// --- Неподдерживаемые (заглушки) ---
static inline long long share_memory(long long target_proc, long long addr, long long size) {
    return __pureos_syscall(SYS_SHARE_MEMORY, target_proc, addr, size);
}
static inline long long pci_device_access(long long bus_slot_func, long long offset) {
    return __pureos_syscall(SYS_PCI_DEVICE_ACCESS, bus_slot_func, offset, 0);
}
static inline long long map_physical_memory(long long phys_addr, long long size) {
    return __pureos_syscall(SYS_MAP_PHYSICAL_MEM, phys_addr, size, 0);
}
static inline long long create_shared_buffer(long long size, long long flags) {
    return __pureos_syscall(SYS_CREATE_SHARED_BUF, size, flags, 0);
}
static inline long long wait_for_vblank(void) {
    return __pureos_syscall(SYS_WAIT_FOR_VBLANK, 0, 0, 0);
}

// ===================================================================
// 🌟 Магические обёртки — юзерленд в одну строку
// ===================================================================

/// Напечатать строку (terminated) в терминал
static inline long long pureos_print(const char* s) {
    long long len = 0;
    while (s[len]) len++;
    return __pureos_syscall(SYS_PRINT, (long long)s, len, 0);
}

/// Напечатать строку с новой строкой
static inline long long pureos_println(const char* s) {
    long long len = 0;
    while (s[len]) len++;
    return __pureos_syscall(SYS_PRINTLN, (long long)s, len, 0);
}

/// Прочитать строку с клавиатуры (блокирующий)
static inline long long pureos_input(char* buf, long long maxlen) {
    return __pureos_syscall(SYS_INPUT, (long long)buf, maxlen, 0);
}

/// Получить системный тик
static inline long long pureos_ticks(void) {
    return __pureos_syscall(SYS_TICKS, 0, 0, 0);
}

/// Очистить экран
static inline long long pureos_cls(void) {
    return __pureos_syscall(SYS_CLS, 0, 0, 0);
}

/// Установить курсор
static inline long long pureos_set_cursor(long long row, long long col) {
    return __pureos_syscall(SYS_SET_CURSOR, row, col, 0);
}

/// Установить цвет текста/фона
static inline long long pureos_color(long long fg, long long bg) {
    return __pureos_syscall(SYS_COLOR, fg, bg, 0);
}

/// Перезагрузка системы через UEFI Runtime Services
static inline long long pureos_reboot(void) {
    return __pureos_syscall(SYS_REBOOT, 0, 0, 0);
}

#endif // SYSCALLS_H
