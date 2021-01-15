%include "paging.mac"

%define BSP_STACK_SIZE (PAGE_SIZE*50)
%define PAGE_HIERARCHY_SIZE (PAGE_SIZE*7)

global PAGE_HIERARCHY
section .bss
PAGE_HIERARCHY:
   align PAGE_SIZE
   resb PAGE_HIERARCHY_SIZE

extern kmain_early

; The stack used before launching the guests. After that, the
; stack will be the one set up in the VMCS
global BSP_STACK_TOP
section .bss
BSP_STACK_BOTTOM:
   align PAGE_SIZE
   resb BSP_STACK_SIZE
BSP_STACK_TOP:

; Global Descriptor Table
global GDT64
global GDT64.code
global GDT64.data
global GDT64.pointer
section .rodata
GDT64:                           ; Global Descriptor Table (64-bit).
.null: equ $ - GDT64         ; The null descriptor.
    dw 0xFFFF                    ; Limit (low).
    dw 0                         ; Base (low).
    db 0                         ; Base (middle)
    db 0                         ; Access.
    db 1                         ; Granularity.
    db 0                         ; Base (high).
.code: equ $ - GDT64         ; The code descriptor.
    dw 0                         ; Limit (low).
    dw 0                         ; Base (low).
    db 0                         ; Base (middle)
    db 10011010b                 ; Access (exec/read).
    db 00100000b                 ; Granularity, 64 bits flag, limit19:16.
    db 0                         ; Base (high).
.data: equ $ - GDT64         ; The data descriptor.
    dw 0                         ; Limit (low).
    dw 0                         ; Base (low).
    db 0                         ; Base (middle)
    db 10010010b                 ; Access (read/write).
    db 00100000b                 ; Granularity.
    db 0                         ; Base (high).
.pointer:                    ; The GDT-pointer.
    dw $ - GDT64 - 1             ; Limit.
    dq GDT64                     ; Base.

; For external usage, create a symbol that is the _address_ of the GDT and the
; value of each segment offset
global GDT64_ADDR
GDT64_ADDR:
     dq GDT64

global GDT64_CODE
GDT64_CODE:
     dq GDT64.code

global GDT64_DATA
GDT64_DATA:
     dq GDT64.data

global IS_MULTIBOOT_BOOT
IS_MULTIBOOT_BOOT:
     db 0

global IS_MULTIBOOT2_BOOT
IS_MULTIBOOT2_BOOT:
     db 0

[BITS 32]

section .text.map_page_directory
map_page_directory:
    push ecx
    mov ecx, 0
.loop:
    mov ebx, eax
    or ebx, PAGE_PRESENT | PAGE_WRITE | PAGE_BIG
    mov [edi], ebx
    add eax, HUGE_PAGE_SIZE
    add edi, 8
    inc ecx

    cmp ecx, 512
    jb .loop

    pop ecx
    ret

global _start
section .text._start
_start:
    lea esp, [BSP_STACK_TOP]
    lea edi, [PAGE_HIERARCHY]

    push ebx


    cmp eax, 0x36d76289         ; Multiboot2 indicator
    je .multiboot2_boot_type
    mov byte [IS_MULTIBOOT_BOOT], 1
    jmp .post_boot_type

.multiboot2_boot_type:
    mov byte [IS_MULTIBOOT2_BOOT], 1

.post_boot_type:

    ; Zero out the buffer.
    ; Since we are doing a rep stosd, count should be bytes/4.
    push edi                           ; REP STOSD alters DI.
    mov ecx, PAGE_HIERARCHY_SIZE/4
    xor eax, eax
    cld
    rep stosd
    pop edi                            ; Get DI back.

    ; Build the Page Map Level 4.
    ; es:di points to the Page Map Level 4 table.
    lea eax, [edi + PAGE_SIZE]          ; Put the address of the Page Directory Pointer Table in to EAX.
    or eax, PAGE_PRESENT | PAGE_WRITE   ; Or EAX with the flags - present flag, writable flag.
    mov [edi], eax                      ; Store the value of EAX as the first PML4E.

    ; Build the Page Directory Pointer Table.
    lea eax, [edi + PAGE_SIZE*2]         ; Put the address of the Page Directory in to EAX.
    or eax, PAGE_PRESENT | PAGE_WRITE    ; Or EAX with the flags - present flag, writable flag.
    mov [edi + PAGE_SIZE], eax           ; Store the value of EAX as the first PDPTE.

    ; The following blocks fill out the other PDPTEs for the 4GB mapping
    lea eax, [edi + PAGE_SIZE*3]
    or eax, PAGE_PRESENT | PAGE_WRITE
    mov [edi + PAGE_SIZE + 8], eax

    lea eax, [edi + PAGE_SIZE*4]
    or eax, PAGE_PRESENT | PAGE_WRITE
    mov [edi + PAGE_SIZE + 8*2], eax

    lea eax, [edi + PAGE_SIZE*5]
    or eax, PAGE_PRESENT | PAGE_WRITE
    mov [edi + PAGE_SIZE + 8*3], eax

    ; Build the Page Directory.
    push edi
    mov edx, edi

    mov eax, 0
    lea edi, [edx+PAGE_SIZE*2]
    call map_page_directory

    mov eax, (HUGE_PAGE_SIZE*512)
    lea edi, [edx+PAGE_SIZE*3]
    call map_page_directory

    mov eax, (HUGE_PAGE_SIZE*512)*2
    lea edi, [edx+PAGE_SIZE*4]
    call map_page_directory

    mov eax, (HUGE_PAGE_SIZE*512)*3
    lea edi, [edx+PAGE_SIZE*5]
    call map_page_directory
    pop edi

    ; Setup PIC
    mov al, 0x11
    out 0x20, al
    out 0xa0, al

    ; Setup offsets
    mov al, 0x20
    out 0x21, al
    mov al, 0x28
    out 0xa1, al

    mov al, 0x4
    out 0x21, al
    mov al, 0x2
    out 0xa1, al

    mov al, 0x1
    out 0x21, al
    out 0xa1, al

    ;; Mask everything so the I/O APIC will work
    mov al, 0xff
    out 0x21, al
    mov al, 0xff
    out 0xA1, al

    ;; ACK any pending interrupts
    mov al, 0x20
    out 0x20, al
    out 0xa0, al

    ; Enter long mode.
    mov eax, 10100000b                ; Set the PAE and PGE bit.
    mov cr4, eax

    mov edx, edi                      ; Point CR3 at the PML4.
    mov cr3, edx

    mov ecx, 0xC0000080               ; Read from the EFER MSR.
    rdmsr

    or eax, 0x00000100                ; Set the LME bit.
    wrmsr

    mov ebx, cr0                      ; Activate long mode -
    or ebx,0x80000001                 ; - by enabling paging and protection simultaneously.
    mov cr0, ebx

    lgdt [GDT64.pointer]              ; Load GDT.Pointer defined below.

    pop edi                           ; Pop the multiboot2 addr to pass to kmain

    jmp GDT64.code:trampoline         ; Load CS with 64 bit segment and flush the instruction cache

[BITS 64]
trampoline:
    ; Update these after the new CS to avoid issues setting SS
    mov eax, GDT64.data
    mov ds, eax
    mov es, eax
    mov ss, eax

    ; Ensure GS/FS are zeroed
    xor edx, edx
    mov gs, edx
    mov fs, edx

    jmp kmain_early
