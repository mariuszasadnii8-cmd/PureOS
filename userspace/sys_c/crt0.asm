bits 64
global _start
extern main

section .text
_start:
    call main
    
    ; Системный вызов exit (sys_no = 6)
    mov rdi, rax    ; код возврата из main
    mov rax, 6      ; sys_exit
    syscall
