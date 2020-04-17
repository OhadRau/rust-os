.global context_save
context_save:
    // Save the remaining context to the stack.

    stp x26, x27, [SP, #-16]!
    stp x24, x25, [SP, #-16]!
    stp x22, x23, [SP, #-16]!
    stp x20, x21, [SP, #-16]!
    stp x18, x19, [SP, #-16]!
    stp x16, x17, [SP, #-16]!
    stp x14, x15, [SP, #-16]!
    stp x12, x13, [SP, #-16]!
    stp x10, x11, [SP, #-16]!
    stp x8,  x9,  [SP, #-16]!
    stp x6,  x7,  [SP, #-16]!
    stp x4,  x5,  [SP, #-16]!
    stp x2,  x3,  [SP, #-16]!
    stp x0,  x1,  [SP, #-16]!

    stp q30, q31, [SP, #-32]!
    stp q28, q29, [SP, #-32]!
    stp q26, q27, [SP, #-32]!
    stp q24, q25, [SP, #-32]!
    stp q22, q23, [SP, #-32]!
    stp q20, q21, [SP, #-32]!
    stp q18, q19, [SP, #-32]!
    stp q16, q17, [SP, #-32]!
    stp q14, q15, [SP, #-32]!
    stp q12, q13, [SP, #-32]!
    stp q10, q11, [SP, #-32]!
    stp q8,  q9,  [SP, #-32]!
    stp q6,  q7,  [SP, #-32]!
    stp q4,  q5,  [SP, #-32]!
    stp q2,  q3,  [SP, #-32]!
    stp q0,  q1,  [SP, #-32]!

    mrs x0, TPIDR_EL0
    mrs x1, SP_EL0
    mrs x2, SPSR_EL1
    mrs x3, ELR_EL1
    mrs x4, TTBR0_EL1
    mrs x5, TTBR1_EL1

    stp x1, x0, [SP, #-16]!
    stp x3, x2, [SP, #-16]!
    stp x4, x5, [SP, #-16]!

    // Save LR so it doesn't get overwritten by function call
    mov x28, lr

    // Call handle_exception(info, esr, tf)
    mov x0, x29      // info
    mrs x1, esr_el1  // esr
    mov x2, sp       // tf
    bl handle_exception

    mov lr, x28

.global context_restore
context_restore:
    // Restore the context from the stack.

    ldp x4, x5, [SP], #16
    ldp x3, x2, [SP], #16
    ldp x1, x0, [SP], #16

    msr TPIDR_EL0, x0
    msr SP_EL0, x1
    msr SPSR_EL1, x2
    msr ELR_EL1, x3
    msr TTBR0_EL1, x4
    msr TTBR1_EL1, x5

    // Wait for memory accesses to complete
    dsb ishst
    tlbi vmalle1
    dsb ish
    isb

    ldp q0,  q1,  [SP], #32
    ldp q2,  q3,  [SP], #32
    ldp q4,  q5,  [SP], #32
    ldp q6,  q7,  [SP], #32
    ldp q8,  q9,  [SP], #32
    ldp q10, q11, [SP], #32
    ldp q12, q13, [SP], #32
    ldp q14, q15, [SP], #32
    ldp q16, q17, [SP], #32
    ldp q18, q19, [SP], #32
    ldp q20, q21, [SP], #32
    ldp q22, q23, [SP], #32
    ldp q24, q25, [SP], #32
    ldp q26, q27, [SP], #32
    ldp q28, q29, [SP], #32
    ldp q30, q31, [SP], #32

    ldp x0,  x1,  [SP], #16
    ldp x2,  x3,  [SP], #16
    ldp x4,  x5,  [SP], #16
    ldp x6,  x7,  [SP], #16
    ldp x8,  x9,  [SP], #16
    ldp x10, x11, [SP], #16
    ldp x12, x13, [SP], #16
    ldp x14, x15, [SP], #16
    ldp x16, x17, [SP], #16
    ldp x18, x19, [SP], #16
    ldp x20, x21, [SP], #16
    ldp x22, x23, [SP], #16
    ldp x24, x25, [SP], #16
    ldp x26, x27, [SP], #16

    ret

.macro HANDLER source, kind
    .align 7
    stp     lr, xzr, [SP, #-16]!
    stp     x28, x29, [SP, #-16]!
    
    mov     x29, \source
    movk    x29, \kind, LSL #16
    bl      context_save
    
    ldp     x28, x29, [SP], #16
    ldp     lr, xzr, [SP], #16
    eret
.endm

.equ sync, 0
.equ irq, 1
.equ fiq, 2
.equ serror, 3

.equ currentSpEl0, 0
.equ currentSpElx, 1
.equ lowerAArch64, 2
.equ lowerAArch32, 3
    
.align 11
.global vectors
vectors:
    // Same exception level (sync, IRQ, FIQ, SError) w/ source = 0
    // Same exception level (sync, IRQ, FIQ, SError) w/ source <> 0
    // Source is at lower EL on AArch64 (sync, IRQ, FIQ, SError)
    // Source is at lower EL on AArch32 (sync, IRQ, FIQ, SError)

    HANDLER currentSpEl0, sync
    HANDLER currentSpEl0, irq
    HANDLER currentSpEl0, fiq
    HANDLER currentSpEl0, serror
    
    HANDLER currentSpElx, sync
    HANDLER currentSpElx, irq
    HANDLER currentSpElx, fiq
    HANDLER currentSpElx, serror

    HANDLER lowerAArch64, sync
    HANDLER lowerAArch64, irq
    HANDLER lowerAArch64, fiq
    HANDLER lowerAArch64, serror

    HANDLER lowerAArch32, sync
    HANDLER lowerAArch32, irq
    HANDLER lowerAArch32, fiq
    HANDLER lowerAArch32, serror