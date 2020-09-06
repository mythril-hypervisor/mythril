extern vmexit_handler
extern vmresume_failure_handler

%macro push_registers 0
    push rax
    push rbx
    push rcx
    push rdx
    push rsi
    push rdi
    push rbp
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15
    mov rax, cr2
    push  rax
%endmacro

%macro pop_registers 0
    pop rax
    mov cr2, rax
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rbp
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rbx
    pop rax
%endmacro

global vmlaunch_wrapper
section .text.vmlaunch_wrapper
vmlaunch_wrapper:
    push_registers

    ;  Clear the host register state to avoid leaking things to the guest
    xor rax, rax
    xor rbx, rbx
    xor rcx, rcx
    mov rdx, 0x406e3 ; TODO: check this
    xor rsi, rsi
    xor rdi, rdi
    xor rbp, rbp
    xor r8, r8
    xor r9, r9
    xor r10, r10
    xor r11, r11
    xor r12, r12
    xor r13, r13
    xor r14, r14
    xor r15, r15
    vmlaunch

    pop_registers

    pushfq
    pop rax
    ret

global vmexit_handler_wrapper
section .text.vmexit_handler_wrapper
vmexit_handler_wrapper:
    push_registers
    mov rdi, rsp

    call vmexit_handler
    pop_registers

    vmresume
    pushfq
    pop rcx
    call vmresume_failure_handler
