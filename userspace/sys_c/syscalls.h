#ifndef SYSCALLS_H
#define SYSCALLS_H

// ===================================================================
// PureOS syscall table (номер в RAX)
// ===================================================================

// --- Базовые (1-15) ---
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

// --- Файловые (16-23) ---
#define SYS_WRITE              16
#define SYS_READ               17
#define SYS_OPEN               18
#define SYS_CLOSE              19
#define SYS_LSEEK              20
#define SYS_STAT               21
#define SYS_DUP                22
#define SYS_FCNTL              23

// --- Магические (24-31) ---
#define SYS_PRINT              24
#define SYS_PRINTLN            25
#define SYS_INPUT              26
#define SYS_TICKS              27
#define SYS_CLS                28
#define SYS_SET_CURSOR         29
#define SYS_COLOR              30
#define SYS_REBOOT             31
#define SYS_PRINT_NUM          32

// --- Графические (33-40) ---
#define SYS_GET_SCREEN_INFO    33
#define SYS_DRAW_PIXEL         34
#define SYS_DRAW_LINE          35
#define SYS_DRAW_RECT          36
#define SYS_DRAW_CIRCLE        37
#define SYS_DRAW_IMAGE         38
#define SYS_CLEAR_SCREEN       39
#define SYS_SET_FONT_SCALE     40

// --- Коды ошибок ---
#define ERR_OK                 0
#define ERR_INVALID_SYSCALL   -1
#define ERR_INVALID_PROCESS   -2
#define ERR_NO_CAPACITY       -3
#define ERR_INVALID_POINTER   -4
#define ERR_OUT_OF_MEMORY     -5
#define ERR_UNSUPPORTED       -38
#define ERR_WOULDBLOCK        -11

// ===================================================================
// Базовый syscall-инлайн
// ===================================================================

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

// ===================================================================
// Управление процессами (1-6)
// ===================================================================

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

// ===================================================================
// IPC (7-9)
// ===================================================================

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

// ===================================================================
// Файловый ввод-вывод (16-23)
// ===================================================================

static inline long long sys_write(long long fd, const void* buf, long long len) {
    return __pureos_syscall(SYS_WRITE, fd, (long long)buf, len);
}
static inline long long sys_read(long long fd, void* buf, long long len) {
    return __pureos_syscall(SYS_READ, fd, (long long)buf, len);
}
static inline long long sys_open(const char* path, long long flags) {
    return __pureos_syscall(SYS_OPEN, (long long)path, flags, 0);
}
static inline long long sys_close(long long fd) {
    return __pureos_syscall(SYS_CLOSE, fd, 0, 0);
}
static inline long long sys_lseek(long long fd, long long offset, long long whence) {
    return __pureos_syscall(SYS_LSEEK, fd, offset, whence);
}
static inline long long sys_stat(const char* path, void* buf) {
    return __pureos_syscall(SYS_STAT, (long long)path, (long long)buf, 0);
}
static inline long long sys_dup(long long fd) {
    return __pureos_syscall(SYS_DUP, fd, 0, 0);
}
static inline long long sys_fcntl(long long fd, long long cmd, long long arg) {
    return __pureos_syscall(SYS_FCNTL, fd, cmd, arg);
}

// ===================================================================
// Магические обёртки (24-32) — быстрый юзерленд
// ===================================================================

static inline long long pureos_print(const char* s) {
    long long len = 0;
    while (s[len]) len++;
    return __pureos_syscall(SYS_PRINT, (long long)s, len, 0);
}
static inline long long pureos_println(const char* s) {
    long long len = 0;
    while (s[len]) len++;
    return __pureos_syscall(SYS_PRINTLN, (long long)s, len, 0);
}
static inline long long pureos_input(char* buf, long long maxlen) {
    return __pureos_syscall(SYS_INPUT, (long long)buf, maxlen, 0);
}
static inline long long pureos_ticks(void) {
    return __pureos_syscall(SYS_TICKS, 0, 0, 0);
}
static inline long long pureos_cls(void) {
    return __pureos_syscall(SYS_CLS, 0, 0, 0);
}
static inline long long pureos_set_cursor(long long row, long long col) {
    return __pureos_syscall(SYS_SET_CURSOR, row, col, 0);
}
static inline long long pureos_color(long long fg, long long bg) {
    return __pureos_syscall(SYS_COLOR, fg, bg, 0);
}
static inline long long pureos_reboot(void) {
    return __pureos_syscall(SYS_REBOOT, 0, 0, 0);
}

// Вывод целого числа (syscall 32, rdi=value, rsi!=0 -> newline)
static inline long long pureos_print_num(long long val, long long newline) {
    return __pureos_syscall(SYS_PRINT_NUM, val, newline, 0);
}

// ===================================================================
// Графические обёртки (33-40)
// ===================================================================

// Информация об экране: width, height, stride, format (4 x u32)
static inline long long pureos_get_screen_info(void* buf) {
    return __pureos_syscall(SYS_GET_SCREEN_INFO, (long long)buf, 0, 0);
}
static inline long long pureos_draw_pixel(long long x, long long y, long long color) {
    return __pureos_syscall(SYS_DRAW_PIXEL, x, y, color);
}
static inline long long pureos_draw_line(long long x1, long long y1, long long x2, long long y2, long long color) {
    long long arg1 = ((x1 & 0xFFFF) << 16) | (y1 & 0xFFFF);
    long long arg2 = ((x2 & 0xFFFF) << 16) | (y2 & 0xFFFF);
    return __pureos_syscall(SYS_DRAW_LINE, arg1, arg2, color);
}
static inline long long pureos_draw_rect(long long x, long long y, long long w, long long h, long long color, long long fill) {
    long long arg1 = ((x & 0xFFFF) << 16) | (y & 0xFFFF);
    long long arg2 = ((w & 0xFFFF) << 16) | (h & 0xFFFF);
    long long arg3 = (fill ? 1 : 0) << 32 | (color & 0xFFFFFFFF);
    return __pureos_syscall(SYS_DRAW_RECT, arg1, arg2, arg3);
}
static inline long long pureos_draw_circle(long long x, long long y, long long radius, long long color, long long fill) {
    long long arg1 = ((x & 0xFFFF) << 16) | (y & 0xFFFF);
    long long arg2 = (fill ? 1 : 0) << 40 | ((radius & 0xFFFF) << 24) | (color & 0xFFFFFF);
    return __pureos_syscall(SYS_DRAW_CIRCLE, arg1, arg2, 0);
}
static inline long long pureos_clear_screen(long long color) {
    return __pureos_syscall(SYS_CLEAR_SCREEN, color, 0, 0);
}

// ===================================================================
// Утилиты
// ===================================================================

// itoa — преобразовать целое в строку
static inline char* pureos_itoa(long long val, char* buf) {
    char* p = buf;
    unsigned long long v;
    if (val < 0) { *p++ = '-'; v = -val; }
    else { v = val; }
    char tmp[20];
    int i = 0;
    if (v == 0) tmp[i++] = '0';
    while (v > 0) { tmp[i++] = '0' + (v % 10); v /= 10; }
    while (i > 0) *p++ = tmp[--i];
    *p = 0;
    return buf;
}

// stdlib-like: strlen
static inline long long pureos_strlen(const char* s) {
    long long n = 0;
    while (s[n]) n++;
    return n;
}

// stdlib-like: strcmp
static inline int pureos_strcmp(const char* a, const char* b) {
    while (*a && *a == *b) { a++; b++; }
    return (unsigned char)*a - (unsigned char)*b;
}

// Цветовые константы
#define COLOR_BLACK   0x000000
#define COLOR_WHITE   0xFFFFFF
#define COLOR_RED     0xFF0000
#define COLOR_GREEN   0x00FF00
#define COLOR_BLUE    0x0000FF
#define COLOR_YELLOW  0xFFFF00
#define COLOR_CYAN    0x00FFFF
#define COLOR_MAGENTA 0xFF00FF

// Стандартные FD
#define STDIN  0
#define STDOUT 1
#define STDERR 2

#endif // SYSCALLS_H
