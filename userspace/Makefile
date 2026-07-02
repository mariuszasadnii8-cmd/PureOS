.PHONY: all clean

CC = x86_64-w64-mingw32-gcc
AS = nasm
ASFLAGS = -f win64
CFLAGS = -Wall -O2 -ffreestanding -nostdlib -fno-pie

all: sys_c/main.o sys_c/crt0.o sys_asm/test_proc.o

sys_c/main.o: sys_c/main.c sys_c/syscalls.h
	$(CC) $(CFLAGS) -c sys_c/main.c -o sys_c/main.o

sys_c/crt0.o: sys_c/crt0.asm
	$(AS) $(ASFLAGS) sys_c/crt0.asm -o sys_c/crt0.o

sys_asm/test_proc.o: sys_asm/test_proc.asm
	$(AS) $(ASFLAGS) sys_asm/test_proc.asm -o sys_asm/test_proc.o

clean:
	rm -f sys_c/main.o sys_c/crt0.o sys_asm/test_proc.o
