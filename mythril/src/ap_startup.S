[BITS 16]

global ap_startup
section .ap_startup
ap_startup:
    cli
    lgdt [GDT32.pointer]
    mov eax, cr0
    or al, 1       ; set PE (Protection Enable) bit in CR0 (Control Register 0)
    mov cr0, eax

    jmp GDT32.code:ap_startup_32

global AP_STARTUP_ADDR
AP_STARTUP_ADDR:
    dw ap_startup

;; ; Global (32-bit) Descriptor Table
GDT32:                           ; Global Descriptor Table (64-bit).
.null: equ $ - GDT32         ; The null descriptor.
    dw 0                         ; Limit (low).
    dw 0                         ; Base (low).
    db 0                         ; Base (middle)
    db 0                         ; Access.
    db 0                         ; Granularity.
    db 0                         ; Base (high).
.code: equ $ - GDT32         ; The code descriptor.
    dw 0xffff                    ; Limit (low).
    dw 0                         ; Base (low).
    db 0                         ; Base (middle)
    db 10011010b                 ; Access (exec/read).
    db 11001111b                 ; Granularity, 64 bits flag, limit19:16.
    db 0                         ; Base (high).
.data: equ $ - GDT32         ; The data descriptor.
    dw 0xffff                    ; Limit (low).
    dw 0                         ; Base (low).
    db 0                         ; Base (middle)
    db 10010010b                 ; Access (read/write).
    db 11001111b                 ; Granularity.
    db 0                         ; Base (high).
.pointer:                    ; The GDT-pointer.
    dw $ - GDT32 - 1             ; Limit.
    dd GDT32                     ; Base.


[BITS 32]
DEFAULT REL

extern ap_entry
extern PAGE_HIERARCHY
extern GDT64.pointer
extern GDT64.code

ap_startup_32:
    mov ax, GDT32.data
    mov ds, ax
    mov ss, ax
    mov es, ax

    ; Ensure FS/GS are zeroed
    xor ax, ax
    mov fs, ax
    mov gs, ax

    ; Enter long mode.
    mov eax, 10100000b                ; Set the PAE and PGE bit.
    mov cr4, eax

    lea edi, [PAGE_HIERARCHY]
    mov cr3, edi

    mov ecx, 0xC0000080               ; Read from the EFER MSR.
    rdmsr

    or eax, 0x00000100                ; Set the LME bit.
    wrmsr

    mov ebx, cr0                      ; Activate long mode -
    or ebx,0x80000001                 ; - by enabling paging and protection simultaneously.
    mov cr0, ebx

    lgdt [GDT64.pointer]

    jmp GDT64.code:ap_startup_64

[BITS 64]

;; This function is a short stub, just so the stack for this ap can be a
;; 64-bit value
ap_startup_64:
    ; Load the stack provided by the bsp
    mov rsp, [AP_STACK_ADDR]

    ; See ap::ApData
    push qword [AP_IDX]

    ; Pass the info we've been given by the BSP up to the rust code
    mov rdi, rsp

    ; Acknowledge that the stack has been used
    mov byte [AP_READY], 1

    ; Jump to rust code
    jmp ap_entry

ALIGN 16
global AP_STACK_ADDR
AP_STACK_ADDR:
    dq 0

global AP_IDX
AP_IDX:
    dq 0

global AP_READY
AP_READY:
    db 0
