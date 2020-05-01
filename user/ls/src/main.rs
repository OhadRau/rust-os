#![feature(asm)]
#![no_std]
#![no_main]

mod cr0;

use kernel_api::syscall::{dir_entry, env_get};
use kernel_api::println;

fn main(args: &[&str]) {
    let mut cwd_buf = [0u8; 128];
    let cwd = match env_get("CWD", &mut cwd_buf) {
        Ok(len) => core::str::from_utf8(&cwd_buf[0..len]).expect("Couldn't parse as UTF-8"),
        Err(e)  => {
            println!("Couldn't read $CWD: {:?}", e);
            return
        },
    };

    let mut full_buf = [0u8; 512];
    let path = if args.len() < 1 {
        "/"
    } else if args[0].chars().nth(0) == Some('/') {
        args[0]
    } else {
        let full_length = cwd.len() + args[0].len();
        if full_length > full_buf.len() {
            println!("Path name is too long: {}{}", cwd, args[0]);
            return
        }
        full_buf[0..cwd.len()].copy_from_slice(&cwd.as_bytes());
        full_buf[cwd.len()..full_length].copy_from_slice(&args[0].as_bytes());
        core::str::from_utf8(&full_buf[0..full_length]).expect("Couldn't concat strings")
    };

    let mut has_next = true;
    let mut offset = 0;
    while has_next {
    let mut buf = [0u8; 512];
        has_next = match dir_entry(path, &mut buf, offset) {
            Ok(has_next) => has_next,
            Err(e) => {
                println!("error while iterating over directory: {:?}", e);
                break
            }
        };
        unsafe { println!("{}", core::str::from_utf8_unchecked(&buf)); }
        offset += 1;
    }
}
