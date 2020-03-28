ENTRY(_start)

SECTIONS {
    . = 0x7E00;

    .ap_startup ALIGN(4K) : ALIGN(4K)
    {
       KEEP(*(.ap_startup .ap_startup.*))
    }

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

    /* Section contents can only be aligned as strictly as the section, and
       we store the page table hierarchy in bss, so align the section to page
       size */
    .bss ALIGN(4K) : ALIGN(4K)
    {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
    }
}
