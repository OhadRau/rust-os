#![feature(alloc_error_handler)]
#![feature(const_fn)]
#![feature(decl_macro)]
#![feature(asm)]
#![feature(global_asm)]
#![feature(optin_builtin_traits)]
#![feature(raw_vec_internals)]
#![feature(panic_info_message)]
#![feature(slice_concat_ext)]
#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]

#[cfg(not(test))]
mod init;

extern crate alloc;

pub mod allocator;
pub mod console;
pub mod fs;
pub mod mutex;
pub mod shell;

use console::kprintln;

use allocator::Allocator;
use fs::FileSystem;

#[cfg_attr(not(test), global_allocator)]
pub static ALLOCATOR: Allocator = Allocator::uninitialized();
pub static FILESYSTEM: FileSystem = FileSystem::uninitialized();

#[no_mangle]
pub extern fn kputs(s: &str) {
    kprintln!("{}", s)
}

fn kmain() -> ! {
    pi::timer::spin_sleep(core::time::Duration::from_millis(500));
    
    unsafe {
        kprintln!("Initializing allocator");
        ALLOCATOR.initialize();
        kprintln!("Initializing filesystem");
        FILESYSTEM.initialize();
    }

    kprintln!("Welcome to cs3210!");
    shell::shell(">");
}
