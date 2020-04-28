#![feature(asm)]
#![feature(alloc_error_handler)]
#![feature(optin_builtin_traits)]
#![no_std]
#![no_main]

mod cr0;
mod allocator;

#[macro_use]
extern crate alloc;
use alloc::string::String;

use shim::io::SeekFrom;
use kernel_api::{print, println, OsResult, OsError};
use kernel_api::syscall::{env_get, fs_open, fs_close, file_seek, file_read};

#[global_allocator]
pub static ALLOCATOR: allocator::Allocator = allocator::Allocator::uninitialized();

fn cat_file(path: String) -> OsResult<()> {
    let fd = fs_open(&path)?;
    file_seek(&fd, SeekFrom::Start(0))?;
    let mut buf = [0u8; 128];
    loop {
        let amt_read = file_read(&fd, &mut buf)?;
        let buf_str = core::str::from_utf8(&buf[0..amt_read]).map_err(|_| OsError::Unknown)?;
        print!("{}", buf_str);
        if amt_read < 128 { break }
        //file_seek(&fd, SeekFrom::Current(amt_read as i64))?;
    }
    fs_close(&fd)
}

fn main(args: &[&str]) {
    let mut cwd_buf = [0u8; 128];
    let cwd = match env_get("CWD", &mut cwd_buf) {
        Ok(len) => core::str::from_utf8(&cwd_buf[0..len]).expect("Couldn't parse as UTF-8"),
        Err(e) => {
            println!("Couldn't read $CWD: {:?}", e);
            return
        },
    };

    for arg in args {
        let full_path = if arg.chars().nth(0) == Some('/') {
            String::from(*arg)
        } else {
            format!("{}{}", cwd, arg)
        };

        match cat_file(full_path) {
            Ok(_) => (),
            Err(e) => println!("Error while reading from {}: {:?}", arg, e),
        }
    }
}
