#![feature(asm)]
#![no_std]
#![no_main]

mod cr0;

fn main(_args: &[&str]) {
    aarch64::brk!(0xFFFF); // we use this brk number to indicate su rather than normal debug shell
}
