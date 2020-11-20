ENTRY(_start)

SECTIONS {
    . = 0x8000;

    .boot :
    {
        /* ensure that the multiboot header is at the beginning */
        KEEP(*(.multiboot_header))
    }

    .ap_startup ALIGN(4K) : ALIGN(4K)
    {
       KEEP(*(.ap_startup .ap_startup.*))
    }

    . = 1M;

    PER_CORE_START = .;
    .per_core :
    {
        *(__PER_CORE*)
    }
    PER_CORE_END = .;

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

    END_OF_BINARY = .;
}
