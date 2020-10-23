DEFAULT REL

[BITS 32]

section .multiboot_header
multiboot_header_start:
align 4
    dd 0x1BADB002                ; magic number (multiboot 1)
    dd 0                         ; flags
    ; checksum
    dd -(0x1BADB002 + 0)
    dd 0
    dd 0
    dd 0
    dd 0
    dd 0

multiboot_header_end:

global MULTIBOOT_HEADER_START
MULTIBOOT_HEADER_START:
    dd multiboot_header_start

global MULTIBOOT_HEADER_END
MULTIBOOT_HEADER_END:
    dd multiboot_header_end
