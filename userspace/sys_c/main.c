#include "syscalls.h"

/// Демонстрация всех возможностей PureOS userspace.
int main() {
    pureos_cls();
    pureos_println("========================================");
    pureos_println("  PureOS Crystal Kernel — Userspace Demo");
    pureos_println("========================================");
    pureos_println("");

    // Test print_num
    pureos_print("System ticks at boot: ");
    pureos_print_num(pureos_ticks(), 1);

    // Test input
    pureos_print("Enter your name: ");
    char name[64];
    long long n = pureos_input(name, 64);
    if (n > 0) {
        name[n] = 0;
        pureos_print("Hello, ");
        pureos_println(name);
    }

    // Test arithmetic via print
    pureos_print("42 + 42 = ");
    pureos_print_num(84, 1);

    // Test file I/O
    pureos_println("");
    pureos_println("--- File I/O Demo ---");
    // Write to /etc/motd
    long long fd = sys_open("/etc/motd", 1);
    if (fd > 0) {
        sys_write(fd, "Written from userspace!\n", 23);
        sys_close(fd);
    }
    // Read it back
    fd = sys_open("/etc/motd", 0);
    if (fd > 0) {
        char buf[128];
        long long r = sys_read(fd, buf, 128);
        if (r > 0) {
            buf[r] = 0;
            pureos_print("motd says: ");
            pureos_println(buf);
        }
        sys_close(fd);
    }

    // Test stat
    char statbuf[32];
    if (sys_stat("/etc/motd", statbuf) == 0) {
        long long* p = (long long*)statbuf;
        long long ino = p[1];
        long long size = *(long long*)(statbuf + 20);
        pureos_print("stat: ino=");
        pureos_print_num(ino, 0);
        pureos_print(" size=");
        pureos_print_num(size, 1);
    }

    // Graphics demo (if supported)
    pureos_println("");
    pureos_println("--- Graphics Demo ---");
    // Draw some pixels
    pureos_draw_pixel(10, 10, COLOR_RED);
    pureos_draw_pixel(20, 10, COLOR_GREEN);
    pureos_draw_pixel(30, 10, COLOR_BLUE);
    // Draw a rectangle
    pureos_draw_rect(50, 50, 100, 60, COLOR_CYAN, 1);

    pureos_println("");
    pureos_println("--- System Info ---");
    pureos_print("PID: ");
    pureos_print_num(0, 1);
    pureos_print("Ticks: ");
    pureos_print_num(pureos_ticks(), 1);

    pureos_println("");
    pureos_println("Userspace demo complete. Type 'barrel' for scripting!");

    while (1) {
        yield_cpu();
    }
    return 0;
}
