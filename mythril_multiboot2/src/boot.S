[BITS 32]
DEFAULT REL

extern kmain

%define PAGE_SIZE 4096
%define HUGE_PAGE_SIZE (4096*512)

%define BSP_STACK_SIZE (PAGE_SIZE*10)
%define PAGE_HIERARCHY_SIZE (PAGE_SIZE*7)

%define PAGE_PRESENT    (1 << 0)
%define PAGE_WRITE      (1 << 1)
%define PAGE_BIG        (1 << 7)

global PAGE_HIERARCHY
section .bss
align PAGE_SIZE
PAGE_HIERARCHY:
   times PAGE_HIERARCHY_SIZE db 0

align PAGE_SIZE
section .bss
BSP_STACK_BOTTOM:
   times BSP_STACK_SIZE db 0
BSP_STACK_TOP:

%macro map_page_directory 2
    push edi
    mov eax, %2
    lea edi, [%1]

%%loop:
    mov ebx, eax
    or ebx, PAGE_PRESENT | PAGE_WRITE | PAGE_BIG
    mov [edi], ebx
    add eax, HUGE_PAGE_SIZE
    add edi, 8

    cmp eax, %2+(HUGE_PAGE_SIZE*512)
    jb %%loop

    pop edi

%endmacro

global _start
section .text._start
_start:
    lea esp, [BSP_STACK_TOP]
    lea edi, [PAGE_HIERARCHY]

    push ebx

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
    map_page_directory edi+PAGE_SIZE*2, 0
    map_page_directory edi+PAGE_SIZE*3, (HUGE_PAGE_SIZE*512)
    map_page_directory edi+PAGE_SIZE*4, (HUGE_PAGE_SIZE*512)*2
    map_page_directory edi+PAGE_SIZE*5, (HUGE_PAGE_SIZE*512)*3

    ; Disable IRQs (probably not necessary)
    mov al, 0xFF                      ; Out 0xFF to 0xA1 and 0x21 to disable all IRQs.
    out 0xA1, al
    out 0x21, al

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

    jmp GDT64.code:kmain              ; Load CS with 64 bit segment and flush the instruction cache

; Global Descriptor Table
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
    db 10101111b                 ; Granularity, 64 bits flag, limit19:16.
    db 0                         ; Base (high).
.data: equ $ - GDT64         ; The data descriptor.
    dw 0                         ; Limit (low).
    dw 0                         ; Base (low).
    db 0                         ; Base (middle)
    db 10010010b                 ; Access (read/write).
    db 00000000b                 ; Granularity.
    db 0                         ; Base (high).
.pointer:                    ; The GDT-pointer.
    dw $ - GDT64 - 1             ; Limit.
    dq GDT64                     ; Base.
