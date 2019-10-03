.section .text
.code64

.global vmexit_handler_wrapper
.extern vmexit_handler

vmexit_handler_wrapper:
    // Save the guest non-vmcs processor states
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
    movq %cr2, %rax
    pushq %rax

    movq %rsp, %rdi
    call vmexit_handler

    // Restore the guest non-vmcs processor state
    popq %rax
    movq %rax, %cr2
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

    // resume the VM
    vmresume

    //FIXME: On error halt, we should jump to a handler
    cli
    hlt
