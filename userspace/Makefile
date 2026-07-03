.PHONY: all clean

CC = x86_64-w64-mingw32-gcc
AS = nasm
LD = x86_64-w64-mingw32-ld

ASFLAGS = -f win64
CFLAGS = -Wall -O2 -ffreestanding -nostdlib -fno-pie -mno-red-zone
LDFLAGS = -T link.ld -nostdlib -static -no-pie

all: hello.elf mandelbrot.elf test_proc.elf

hello.elf: crt0.o main.o
	$(LD) $(LDFLAGS) crt0.o main.o -o hello.elf

mandelbrot.elf: crt0.o mandelbrot.o
	$(LD) $(LDFLAGS) crt0.o mandelbrot.o -o mandelbrot.elf

test_proc.elf: test_proc.o
	$(LD) $(LDFLAGS) test_proc.o -o test_proc.elf

main.o: main.c syscalls.h
	$(CC) $(CFLAGS) -c main.c -o main.o

crt0.o: crt0.asm
	$(AS) $(ASFLAGS) crt0.asm -o crt0.o

mandelbrot.o: mandelbrot.c syscalls.h
	$(CC) $(CFLAGS) -c mandelbrot.c -o mandelbrot.o

test_proc.o: ${PWD}/sys_asm/test_proc.asm
	$(AS) $(ASFLAGS) ${PWD}/sys_asm/test_proc.asm -o test_proc.o

clean:
	rm -f *.o *.elf
