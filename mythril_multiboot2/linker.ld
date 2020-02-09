ENTRY(_start)

SECTIONS {
    . = 1M;

    .boot ALIGN(4K) : ALIGN(4K)
    {
        /* ensure that the multiboot header is at the beginning */
        KEEP(*(.multiboot_header))
    }

    .text :
    {
        *(.text .text.*)
    }

    .rodata :
    {
        *(.rodata .rodata.*)
    }

    .data :
    {
        *(.data .data.*)
    }

    __bss_start = .;
    .bss :
    {
        *(.bss .bss.*)
        *(COMMON)
    }
}
