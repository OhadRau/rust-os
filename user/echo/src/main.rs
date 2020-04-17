#![feature(asm)]
#![no_std]
#![no_main]

mod cr0;

use kernel_api::{print, println};
use kernel_api::syscall::{input, output, fork, exec, wait_pid, getpid, sleep, exit};

fn main() {
    print!("Echo {}> ", getpid());
    loop {
        let ch = input();
        output(ch);
        if ch == '\n' as u8 || ch == '\r' as u8 {
            break;
        } else if ch == '!' as u8 {
            exit();
        }
    }
    println!("");

    println!("Forking {}...", getpid());

    match fork() {
        Ok(0) => println!("Hello from the child"),
        Ok(pid) => {
            println!("Hello from the parent of {}", pid);
            wait_pid(pid);
        },
        Err(e) => println!("Error: {:?}!", e),
    }


    println!("Executing /echo");
    exec("/echo");

    exit()
}
