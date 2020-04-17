#![feature(asm)]
#![feature(global_asm)]
#![feature(alloc_error_handler)]
#![feature(optin_builtin_traits)]
#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]

#[cfg(not(test))]
mod init;
mod allocator;

use xmodem::Xmodem;
use core::time::Duration;
use core::fmt::Write;
use pi;
use allocator::Allocator;
use core::alloc::Layout;

/// Start address of the binary to load and of the bootloader.
const BINARY_START_ADDR: usize = 0x80000;
const BOOTLOADER_START_ADDR: usize = 0x4000000;

/// Pointer to where the loaded binary expects to be laoded.
const BINARY_START: *mut u8 = BINARY_START_ADDR as *mut u8;

/// Free space between the bootloader and the loaded binary's start address.
const MAX_BINARY_SIZE: usize = BOOTLOADER_START_ADDR - BINARY_START_ADDR;

#[cfg_attr(not(test), global_allocator)]
pub static ALLOCATOR: Allocator = Allocator::uninitialized();

/// Branches to the address `addr` unconditionally.
unsafe fn jump_to(addr: *mut u8) -> ! {
    asm!("br $0" : : "r"(addr as usize));
    loop {
        asm!("wfe" :::: "volatile")
    }
}

fn kmain() -> ! {
    unsafe { ALLOCATOR.initialize(); }
    // uart is init with No timeout
    let mut uart = pi::uart::MiniUart::new();
    while !uart.has_byte() {
        uart.write_str("Welcome to the Bootloader!!\n");
    }

    // load or boot
    let mut choice = '0';
    while choice != '1' && choice != '2' {
        uart.write_str("\rDownload new kernel(1) or boot from disk(2)? ");
        choice = uart.read_byte() as char;
        uart.write_byte(choice as u8);
    }

    match choice {
        '1' => download_kern(uart),
        '2' => load_kern_from_disk(uart),
        _ => unimplemented!()
    }

    // verify disk password 
    unsafe { jump_to(BINARY_START) }
}

fn load_kern_from_disk(mut uart: pi::uart::MiniUart) {
    uart.write_str("\nDisk kern!!");
    loop {}
}

// gonna save it to a hardcoded location on boot partition for now
fn download_kern(mut uart: pi::uart::MiniUart) {
    uart.write_str("\nAttempting to download kernel over UART");
    loop {
        let target = unsafe {
            core::slice::from_raw_parts_mut(BINARY_START, MAX_BINARY_SIZE)
        };
        let mut uart = pi::uart::MiniUart::new();
        uart.set_read_timeout(Duration::from_millis(750));
        match Xmodem::receive(uart, target) {
            Ok(size) => {
                save_kern(size);
                break;
            },
            Err(_) => continue
        }
    }
}

fn save_kern(size: usize) {

}

#[alloc_error_handler]
pub fn oom(_layout: Layout) -> ! {
    panic!("OOM");
}
