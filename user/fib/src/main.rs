#![feature(asm)]
#![no_std]
#![no_main]

mod cr0;

use kernel_api::println;
use kernel_api::syscall::{getpid, time, exit, sleep};

fn fib(n: u64) -> u64 {
    match n {
        0 => 1,
        1 => 1,
        n => fib(n - 1) + fib(n - 2),
    }
}

fn main() {
    println!("Started as {} at {:?}...", getpid(), time());

    //let slept = sleep(core::time::Duration::from_millis(10));
    //println!("I slept for {:?} and now it's {:?}", slept, time());

    let rtn = fib(40);

    println!("Ended: Result = {}", rtn);
    exit()
}
