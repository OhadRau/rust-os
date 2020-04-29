#![feature(asm)]
#![no_std]
#![no_main]
#![feature(alloc_error_handler)]
#![feature(optin_builtin_traits)]

mod cr0;
mod allocator;

#[macro_use]
extern crate alloc;
use alloc::string::String;
use alloc::boxed::Box;
use blockdev::mount::MountOptions;
use kernel_api::syscall::{exit, unmount};
use kernel_api::println;


#[global_allocator]
pub static ALLOCATOR: allocator::Allocator = allocator::Allocator::uninitialized();
fn main(args: &[&str]) {
    unsafe { ALLOCATOR.initialize(); }
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
