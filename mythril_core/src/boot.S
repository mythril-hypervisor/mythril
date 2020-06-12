%include "paging.mac"

%define BSP_STACK_SIZE (PAGE_SIZE*50)

; The stack used before launching the guests. After that, the
; stack will be the one set up in the VMCS
global BSP_STACK_TOP
section .bss
BSP_STACK_BOTTOM:
   align PAGE_SIZE
   times BSP_STACK_SIZE db 0
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
