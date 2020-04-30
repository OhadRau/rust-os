#![feature(asm)]
#![no_std]
#![no_main]
mod cr0;

use kernel_api::syscall::{exit, unmount};
use kernel_api::println;

fn main(args: &[&str]) {
    if args.len() < 1 {
        println!("not enough arguments!\nusage: umount <path>");
        return;
    }
    
    let path = args[0];
    match unmount(path) {
        Ok(_) => { println!("unmounted {}", path); },
        Err(_) => ()
    }
    exit();
}
