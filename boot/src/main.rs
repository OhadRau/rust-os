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
use sd::sd::Sd;
use blockdev::mount::MountOptions;
use fat32::vfat::{VFat, VFatHandle, Metadata, File};
use fat32::traits::{FileSystem, Entry, Dir};
use core::fmt::Debug;
use crate::allocator::mutex::Mutex;
#[macro_use]
extern crate alloc;
use alloc::rc::Rc;
use alloc::string::String;
use core::fmt;

// I copied this from kern/src/fs.rs
#[derive(Clone)]
pub struct PiVFatHandle(Rc<Mutex<VFat<Self>>>);

// These impls are *unsound*. We should use `Arc` instead of `Rc` to implement
// `Sync` and `Send` trait for `PiVFatHandle`. However, `Arc` uses atomic memory
// access, which requires MMU to be initialized on ARM architecture. Since we
// have enabled only one core of the board, these unsound impls will not cause
// any immediate harm for now. We will fix this in the future.
unsafe impl Send for PiVFatHandle {}
unsafe impl Sync for PiVFatHandle {}

impl Debug for PiVFatHandle {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "PiVFatHandle")
    }
}

impl VFatHandle for PiVFatHandle {
    fn new(val: VFat<PiVFatHandle>) -> Self {
        PiVFatHandle(Rc::new(Mutex::new(val)))
    }

    fn lock<R>(&self, f: impl FnOnce(&mut VFat<PiVFatHandle>) -> R) -> R {
        f(&mut self.0.lock())
    }
}

/// Start address of the binary to load and of the bootloader.
const BINARY_START_ADDR: usize = 0x80000;
const BOOTLOADER_START_ADDR: usize = 0x4000000;

/// Pointer to where the loaded binary expects to be laoded.
const BINARY_START: *mut u8 = BINARY_START_ADDR as *mut u8;

/// Free space between the bootloader and the loaded binary's start address.
const MAX_BINARY_SIZE: usize = BOOTLOADER_START_ADDR - BINARY_START_ADDR;

const KERNEL_IMG_NAME: &'static str = "real_kernel.bin";

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
    use shim::io::Read;
    use fat32::traits::File;
    
    uart.write_str(&format!("\nLoading kernel from disk at path: /{}...", KERNEL_IMG_NAME));

    let mut fs = init_fs(&mut uart).unwrap();
    if !kern_file_exists(&fs) {
        uart.write_str("no image to load :(");
    }

    let mut kern_fd = open_kern_file(&fs);
    unsafe { kern_fd.read_exact(core::slice::from_raw_parts_mut(BINARY_START, kern_fd.size() as usize)); } 
    uart.write_str("done.\n");
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
        match Xmodem::receive(&mut uart, target) {
            Ok(size) => {
                save_kern(&mut uart, size);
                break;
            },
            Err(_) => continue
        }
    }
}

fn save_kern(mut uart: &mut pi::uart::MiniUart, size: usize) {
    let mut fs = init_fs(&mut uart).unwrap();
       
    let mut root_dir = match (&fs).open("/") {
        Ok(entry) => entry.into_dir().unwrap(),
        _ => return
    };

    if !kern_file_exists(&fs) {
        root_dir.create(Metadata {
            name: String::from(KERNEL_IMG_NAME),
            ..Default::default()
        });
    }

    let mut fd = open_kern_file(&fs);
    // need to implement file shrinking to do this properly
    // this might cause problems if we save a smaller kernel
    uart.write_str(&format!("writing kernel to disk ({} bytes)...", size));
    unsafe {
        use shim::io::Write;
        fd.write(core::slice::from_raw_parts(BINARY_START, size)); 
    }
    fs.flush();
    uart.write_str("done.\n");
}

fn init_fs(mut uart: &mut pi::uart::MiniUart) -> Option<PiVFatHandle> {
    let sd = unsafe { Sd::new().expect("Unable to init SD card") };
    let fs = match VFat::<PiVFatHandle>::from(sd, 1, MountOptions::Normal) {
        Ok(handle) => handle,
        Err(e) => {
            uart.write_str(&format!("error initializing FS: {:?}", e));
            return None;
        }
    };

    Some(fs)
}

fn kern_file_exists(fs: &PiVFatHandle) -> bool {
    // we should add something to do this automatically in the FS
    let mut root_dir = match (&fs).open("/") {
        Ok(entry) => entry.into_dir().unwrap(),
        _ => return false
    };

    let mut kern_file_exists = false;
    for entry in root_dir.entries().unwrap() {
        if entry.name().eq(KERNEL_IMG_NAME) {
            kern_file_exists = true; 
            break;
        }
    }

    kern_file_exists
}


fn open_kern_file(fs: &PiVFatHandle) -> File<PiVFatHandle> {
    let mut img_path = String::from("/");
    img_path.push_str(KERNEL_IMG_NAME);
    let mut fd = fs.open_file(img_path).expect("Couldn't open file for writing");
    fd
}

#[alloc_error_handler]
pub fn oom(_layout: Layout) -> ! {
    panic!("OOM");
}