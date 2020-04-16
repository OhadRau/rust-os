#[repr(C)]
#[derive(Default, Copy, Clone, Debug)]
pub struct TrapFrame {
    pub ttbr0: u64,
    pub ttbr1: u64,
    pub elr: u64,
    pub spsr: u64,
    pub sp: u64,
    pub tpidr: u64,
    pub qs: [u128; 32],
    pub xs: [u64; 30],
    pub lr: u64,
    pub xzr: u64,
}
