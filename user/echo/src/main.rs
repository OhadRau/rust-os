#![feature(asm)]
#![no_std]
#![no_main]

mod cr0;

use kernel_api::println;
use kernel_api::syscall::{input, output, exit};

fn main() {
    println!("Echo> ");
    loop {
        let ch = input();
        output(ch);
        if ch as char == '.' {
            break;
        }
    }
    exit()
}
