#![feature(asm)]
#![no_std]
#![no_main]

mod cr0;

use kernel_api::syscall::{exit, mount};
use kernel_api::println;

fn main(args: &[&str]) {
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

    let mut encrypted = false;
    if args.len() > 2 && args.len() != 3 {
        println!("incorrect arguments!\nusage: mount <part> <path> -p");
        return;
    } else if args.len() > 2 {
        if args[2].eq_ignore_ascii_case("-p") {
            encrypted = true;
        } else {
            println!("unknown flag: {}", args[2]);
            return;
        }
    }


    mount(part_num, args[1], encrypted); 
    //println!("unable to mount {} as {}. See above for error description.", part_num, args[1]);

    exit();
}
