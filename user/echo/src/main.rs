#![feature(asm)]
#![no_std]
#![no_main]

mod cr0;

use kernel_api::{print, println};
use kernel_api::syscall::{input, output, fork, getpid, sleep, exit};

fn main() {
    print!("Echo {}> ", getpid());
    loop {
        let ch = input();
        output(ch);
        if ch == '\n' as u8 || ch == '\r' as u8 {
            break;
        }
    }
    println!("");

    println!("Forking {}...", getpid());

    match fork() {
        Ok(0) => println!("Hello from the child"),
        Ok(pid) => println!("Hello from the parent of {}", pid),
        Err(e) => println!("Error: {:?}!", e),
    }

    sleep(core::time::Duration::from_millis(10));

    exit()
}
