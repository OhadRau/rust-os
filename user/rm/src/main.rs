#![feature(asm)]
#![no_std]
#![no_main]

mod cr0;

use kernel_api::println;
use kernel_api::syscall::{env_get, fs_delete};

fn main(args: &[&str]) {
    let mut cwd_buf = [0u8; 128];
    let cwd_string = unsafe { core::str::from_utf8_unchecked_mut(&mut cwd_buf) };
    let cwd = match env_get("CWD", cwd_string) {
        Ok(len) => &cwd_string[0..len],
        Err(e)  => {
            println!("Couldn't read $CWD: {:?}", e);
            return
        },
    };

    for arg in args {
        let mut full_buf = [0u8; 128];
        let path = if arg.chars().nth(0) == Some('/') {
            arg
        } else {
            let full_length = cwd.len() + arg.len();
            if full_length > full_buf.len() {
                println!("Path name is too long: {}{}", cwd, arg);
                continue
            }
            full_buf[0..cwd.len()].copy_from_slice(&cwd.as_bytes());
            full_buf[cwd.len()..full_length].copy_from_slice(&arg.as_bytes());
            core::str::from_utf8(&full_buf[0..full_length]).expect("Couldn't concat strings")
        };
        if let Err(e) = fs_delete(path) {
            println!("Error while deleting file {}: {:?}", arg, e);
        }
    }
}
