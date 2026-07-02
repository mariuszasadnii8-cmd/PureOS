OUTPUT_FORMAT(elf64-x86-64)
ENTRY(_start)

PHDRS
{
    text   PT_LOAD FLAGS(5);  /* PF_R | PF_X */
    rodata PT_LOAD FLAGS(4);  /* PF_R */
    data   PT_LOAD FLAGS(6);  /* PF_R | PF_W */
    bss    PT_LOAD FLAGS(6);  /* PF_R | PF_W */
}

SECTIONS
{
    /* 32 MiB — выше зоны UEFI (OVMF использует 0–4 MiB + boot services) */
    . = 0x2000000;

    .text : ALIGN(4K) {
        KEEP(*(.text._start))
        KEEP(*(.text.startup .text.startup.*))
        *(.text .text.*)
    } :text

    .rodata : ALIGN(4K) {
        *(.rodata .rodata.*)
        *(.rdata .rdata.*)
    } :rodata

    .data : ALIGN(4K) {
        *(.data .data.*)
        *(.got .got.*)
        *(.got.plt)
        *(.plt)
    } :data

    .bss : ALIGN(4K) {
        *(COMMON)
        *(.bss .bss.*)
    } :bss

    /DISCARD/ : {
        *(.comment)
        *(.comment.*)
        *(.eh_frame)
        *(.eh_frame.*)
        *(.note)
        *(.note.*)
        *(.debug)
        *(.debug.*)
        *(.llvm_addrsig)
        *(.interp)
        *(.dynsym)
        *(.dynstr)
        *(.hash)
    }
}
