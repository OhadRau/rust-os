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
use kernel_api::syscall::{exit, mount};
use kernel_api::println;


#[global_allocator]
pub static ALLOCATOR: allocator::Allocator = allocator::Allocator::uninitialized();
fn main(args: &[&str]) {
    //unsafe { ALLOCATOR.initialize(); }
    if args.len() < 2 {
        println!("not enough arguments!\nusage: mount <part> <path> -p <pw>");
        return;
    }

    let part_num: u64 = match args[0].parse() {
        Ok(num) => num,
        Err(_) => {
            println!("invalid partition number");
            return;
        }
    };

    /*let abs_path = match get_abs_path(cwd, args[1]) {
        Some(p) => p,
        None => return
    };*/

    //let mut mount_opts = Box::new(MountOptions::Normal);
    if args.len() > 2 && args.len() != 4 {
        println!("incorrect arguments!\nusage: mount <part> <path> -p <pw>");
        return;
    } else if args.len() > 2 {
        if args[2].eq_ignore_ascii_case("-p") {
            //mount_opts = Box::new(MountOptions::Encrypted(Some(String::from(args[3]))));
        } else {
            println!("unknown flag: {}", args[2]);
            return;
        }
    }


    //mount(part_num, args[1], mount_opts.as_mut() as *mut MountOptions as u64);
    mount(part_num, args[1], 0);

    exit();
}
