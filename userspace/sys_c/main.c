#include "syscalls.h"

int main() {
    pureos_cls();
    pureos_println("Hello from PureOS userspace!");
    pureos_print("Enter your name: ");

    char name[64];
    long long n = pureos_input(name, 64);
    if (n > 0) {
        name[n] = 0;
        pureos_print("Hello, ");
        pureos_println(name);
    }

    pureos_print("System ticks: ");
    // Вывод числа — пока нет цифровой обёртки в C, используем цикл
    long long t = pureos_ticks();
    // В следующей версии добавим pureos_print_num()

    pureos_println("Type 'barrel' in shell for scripting!");

    while (1) {
        pureos_ticks();
    }
    return 0;
}
