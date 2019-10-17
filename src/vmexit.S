.section .text
.code64

.global vmexit_handler_wrapper
.global vmlaunch_wrapper

.extern vmexit_handler
.extern vmresume_failure_handler

.macro push_reg_state
    pushq %rax
    pushq %rbx
    pushq %rcx
    pushq %rdx
    pushq %rsi
    pushq %rdi
    pushq %rbp
    pushq %r8
    pushq %r9
    pushq %r10
    pushq %r11
    pushq %r12
    pushq %r13
    pushq %r14
    pushq %r15
.endm

.macro pop_reg_state
    popq %r15
    popq %r14
    popq %r13
    popq %r12
    popq %r11
    popq %r10
    popq %r9
    popq %r8
    popq %rbp
    popq %rdi
    popq %rsi
    popq %rdx
    popq %rcx
    popq %rbx
    popq %rax
.endm

vmlaunch_wrapper:
    push_reg_state
    //  Clear the host register state to avoid leaking things to the guest
    xor %rax, %rax
    xor %rbx, %rbx
    xor %rcx, %rcx
    xor %rdx, %rdx
    xor %rsi, %rsi
    xor %rdi, %rdi
    xor %rbp, %rbp
    xor %r8, %r8
    xor %r9, %r9
    xor %r10, %r10
    xor %r11, %r11
    xor %r12, %r12
    xor %r13, %r13
    xor %r14, %r14
    xor %r15, %r15
    vmlaunch
    pop_reg_state
    pushfq
    popq %rax
    ret

vmexit_handler_wrapper:
    // Save the guest non-vmcs processor states
    push_reg_state
    movq %cr2, %rax
    pushq %rax
    movq %rsp, %rcx
    call vmexit_handler

    // Restore the guest non-vmcs processor state
    popq %rax
    movq %rax, %cr2
    pop_reg_state

    // resume the VM
    vmresume

    pushfq
    popq %rcx
    call vmresume_failure_handler
