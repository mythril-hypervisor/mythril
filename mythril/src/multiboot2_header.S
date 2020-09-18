DEFAULT REL

[BITS 32]

section .multiboot_header
header_start:
    dd 0xe85250d6                ; magic number (multiboot 2)
    dd 0                         ; architecture 0 (protected mode i386)
    dd header_end - header_start ; header length
    ; checksum
    dd 0x100000000 - (0xe85250d6 + 0 + (header_end - header_start))

    ; insert optional multiboot tags here

    ; required end tag
    dw 0    ; type
    dw 0    ; flags
    dd 8    ; size
header_end:

global MULTIBOOT2_HEADER_START
MULTIBOOT2_HEADER_START:
    dd header_start

global MULTIBOOT2_HEADER_END
MULTIBOOT2_HEADER_END:
    dd header_end
