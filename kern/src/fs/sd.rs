use core::time::Duration;
use shim::io;
use shim::ioerr;
use alloc::boxed::Box;
use crate::console::{kprintln, kprint};
use crate::ALLOCATOR;
use core::mem;
use core::alloc::Layout;
use core::alloc::GlobalAlloc;
use fat32::traits::BlockDevice;
use pi::timer;
use core::fmt;
use shim::{const_assert_eq, const_assert_size};

extern "C" {
    /// zeros the memory for the static sd descriptor
    fn sdInit(); 

    /// does the actual SD card initialization
    fn sdInitCard() -> i64; 

    /// transfers num_blocks blocks to the SD card from buffer
    /// addr: BYTE address of where to read from/write to
    /// num_blocks: the number of blocks to transfer
    /// buffer: buffer from which we read data/into which we right data
    /// write: controls whether this transfer is read or write. 0 for READ, 1 for WRITE
    fn sdTransferBlocks(addr: u64, num_blocks: i32, buffer: *mut u8, write: i32) -> i64;
}

#[no_mangle]
pub extern "C" fn uart_putc(c: char) {
    // this is a binding for the SD library that allows it to print to 
    // our console
    kprint!("{}", c);
}

/// A handle to an SD card controller.
#[derive(Debug)]
pub struct Sd {}

impl Sd {
    /// Initializes the SD card controller and returns a handle to it.
    /// The caller should assure that the method is invoked only once during the
    /// kernel initialization. We can enforce the requirement in safe Rust code
    /// with atomic memory access, but we can't use it yet since we haven't
    /// written the memory management unit (MMU).
    pub unsafe fn new() -> Result<Sd, io::Error> {
        sdInit();
        kprintln!("\nsd init ret: {}", sdInitCard());

        Ok(Sd {})
    }
}

impl BlockDevice for Sd {
    /// Reads sector `n` from the SD card into `buf`. On success, the number of
    /// bytes read is returned.
    ///
    /// # Errors
    ///
    /// An I/O error of kind `InvalidInput` is returned if `buf.len() < 512` or
    /// `n > 2^31 - 1` (the maximum value for an `i32`).
    ///
    /// An error of kind `TimedOut` is returned if a timeout occurs while
    /// reading from the SD card.
    ///
    /// An error of kind `Other` is returned for all other errors.
    fn read_sector(&mut self, n: u64, buf: &mut [u8]) -> io::Result<usize> {
        if n > 0x7FFFFFFF {
            return ioerr!(InvalidInput, "n is too large");
        }
        if buf.len() < 512 {
            return ioerr!(InvalidInput, "buf.len() must be at least 512");
        }

        let buf_ptr = buf.as_mut_ptr();

        // multiply n by 512 bc sdTransferBlocks expects a byte address
        match unsafe { sdTransferBlocks(n * 512, 1, buf_ptr, 0) } {
            0 => {
                return Ok(n as usize);
            }
            err => { 
                kprintln!("read error occured: {}", err);
                ioerr!(BrokenPipe, "unknown sd error occurred")
            }
        }
    }

    fn write_sector(&mut self, n: u64, buf: &[u8]) -> io::Result<usize> {
        if n > 0x7FFFFFFF {
            return ioerr!(InvalidInput, "n is too large");
        }
        if buf.len() < 512 {
            return ioerr!(InvalidInput, "buf.len() must be at least 512");
        }

        let buf_ptr = buf.clone().as_ptr(); 

        //panic!("write");
        kprintln!("writing sector {}", n);
        //assert!((buf_ptr as u64) % 8 == 0, "buf not aligned!!");
        // multiply n by 512 bc sdTransferBlocks expects a byte address
        match unsafe { sdTransferBlocks(n * 512, 1, buf_ptr as *mut u8, 1) } {
            0 => {
                return Ok(n as usize);
            }
            err => { 
                kprintln!("write error occured: {}", err);
                ioerr!(BrokenPipe, "unknown sd error occurred")
            }
        }
    }
}
