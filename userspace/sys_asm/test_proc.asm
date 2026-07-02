bits 64
global _start

section .text
_start:
    ; Пример простого процесса на чистом ASM
    ; Системный вызов create_shared_buffer(size=1024, flags=3)
    mov rax, 13     ; sys_create_shared_buffer
    mov rdi, 1024   ; size
    mov rsi, 3      ; flags (Read/Write)
    mov rdx, 0      ; unused
    syscall
    
    mov rbx, rax    ; сохранить дескриптор буфера
    
.loop:
    ; Системный вызов wait_for_vblank()
    mov rax, 14     ; sys_wait_for_vblank
    xor rdi, rdi
    xor rsi, rsi
    xor rdx, rdx
    syscall
    
    ; Здесь может быть код рендеринга во фреймбуфер...
    
    jmp .loop
    
    ; Системный вызов exit (sys_no = 6)
    mov rax, 6      ; sys_exit
    mov rdi, 0      ; код возврата
    syscall
