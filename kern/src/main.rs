#![feature(alloc_error_handler)]
#![feature(const_fn)]
#![feature(decl_macro)]
#![feature(asm)]
#![feature(global_asm)]
#![feature(optin_builtin_traits)]
#![feature(ptr_internals)]
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
pub mod param;
pub mod process;
pub mod traps;
pub mod vm;

use console::kprintln;

use allocator::Allocator;
use fs::FileSystem;
use process::GlobalScheduler;
use traps::irq::Irq;
use vm::VMManager;

#[cfg_attr(not(test), global_allocator)]
pub static ALLOCATOR: Allocator = Allocator::uninitialized();
pub static FILESYSTEM: FileSystem = FileSystem::uninitialized();
pub static SCHEDULER: GlobalScheduler = GlobalScheduler::uninitialized();
pub static VMM: VMManager = VMManager::uninitialized();
pub static IRQ: Irq = Irq::uninitialized();

#[no_mangle]
pub extern fn kputs(s: &str) {
    kprintln!("{}", s)
}

fn kmain() -> ! {
    pi::timer::spin_sleep(core::time::Duration::from_millis(500));
    
    unsafe {
        kprintln!("In Exception Level {}", aarch64::current_el());
        
        kprintln!("Initializing allocator");
        ALLOCATOR.initialize();
        
        kprintln!("Initializing filesystem");
        FILESYSTEM.initialize();

        kprintln!("Initializing IRQ");
        IRQ.initialize();

        kprintln!("Initializing VM");
        VMM.initialize();

        kprintln!("Initializing scheduler");
        //SCHEDULER.initialize();
        //SCHEDULER.start();
    }

    loop {
        kprintln!("Welcome to cs3210!");
        shell::shell(">");
    }
}
