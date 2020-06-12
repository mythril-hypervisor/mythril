%include "paging.mac"

[BITS 32]
DEFAULT REL

extern kmain
extern GDT64.pointer
extern GDT64.code
extern GDT64.data
extern BSP_STACK_TOP

%define PAGE_HIERARCHY_SIZE (PAGE_SIZE*7)

global PAGE_HIERARCHY
section .bss
PAGE_HIERARCHY:
   align PAGE_SIZE
   times PAGE_HIERARCHY_SIZE db 0

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

    jmp GDT64.code:trampoline         ; Load CS with 64 bit segment and flush the instruction cache

[BITS 64]
trampoline:
    ; Update these after the new CS to avoid issues setting SS
    mov eax, GDT64.data
    mov ds, eax
    mov es, eax
    mov gs, eax
    mov ss, eax

    ; FS stores the per-core index, which is always 0 for the BSP
    mov edx, 0
    mov fs, edx

    jmp kmain
