#![feature(asm)]
#![no_std]
#![no_main]

mod cr0;

use kernel_api::println;
use kernel_api::syscall::{getpid, time};

fn fib(n: u64) -> u64 {
    match n {
        0 => 1,
        1 => 1,
        n => fib(n - 1) + fib(n - 2),
    }
}

fn main(args: &[&str]) {
    println!("Started as {} at {:?}...", getpid(), time());

    match args[0].parse::<u64>() {
        Ok(num) => {
            let rtn = fib(num);
            println!("Ended: Result = {}", rtn);
        },
        Err(e) => println!("Err: {:?}", e),
    }
}
