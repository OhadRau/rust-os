#![feature(asm)]
#![no_std]
#![no_main]
mod cr0;

use kernel_api::syscall::{exit, lsblk};

fn main(_args: &[&str]) {
    lsblk(); 
    exit();
}
