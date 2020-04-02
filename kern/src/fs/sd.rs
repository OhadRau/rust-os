use core::time::Duration;
use shim::io;
use shim::ioerr;
use crate::mutex::Mutex;

use fat32::traits::BlockDevice;

extern "C" {
    /// A global representing the last SD controller error that occured.
    static sd_err: i64;

    /// Initializes the SD card controller.
    ///
    /// Returns 0 if initialization is successful. If initialization fails,
    /// returns -1 if a timeout occured, or -2 if an error sending commands to
    /// the SD controller occured.
    fn sd_init() -> i32;

    /// Reads sector `n` (512 bytes) from the SD card and writes it to `buffer`.
    /// It is undefined behavior if `buffer` does not point to at least 512
    /// bytes of memory. Also, the caller of this function should make sure that
    /// `buffer` is at least 4-byte aligned.
    ///
    /// On success, returns the number of bytes read: a positive number.
    ///
    /// On error, returns 0. The true error code is stored in the `sd_err`
    /// global. `sd_err` will be set to -1 if a timeout occured or -2 if an
    /// error sending commands to the SD controller occured. Other error codes
    /// are also possible but defined only as being less than zero.
    fn sd_readsector(n: i32, buffer: *mut u8) -> i32;
}

// Define a `#[no_mangle]` `wait_micros` function for use by `libsd`.
// The `wait_micros` C signature is: `void wait_micros(unsigned int);`
#[no_mangle]
fn wait_micros(mics: u32) {
    // If we don't multiply by 100, it fails to load on my SanDisk SD card
    // Shoutout to Will Gulian for the tip
    pi::timer::spin_sleep(Duration::from_micros(mics as u64 * 100));
}

static mut LOCK: Mutex<()> = Mutex::new(());

/// A handle to an SD card controller.
#[derive(Debug)]
pub struct Sd;

impl Sd {
    /// Initializes the SD card controller and returns a handle to it.
    /// The caller should assure that the method is invoked only once during the
    /// kernel initialization. We can enforce the requirement in safe Rust code
    /// with atomic memory access, but we can't use it yet since we haven't
    /// written the memory management unit (MMU).
    pub unsafe fn new() -> Result<Sd, io::Error> {
        LOCK.lock();
        match sd_init() {
             0 => Ok(Sd),
            -1 => ioerr!(TimedOut, "Timeout occured while initializing SD card"),
            -2 => ioerr!(Other, "Unable to send commands to SD card"),
            _ => ioerr!(Other, "Unknown SD card error occured"),
        }
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
        use core::alloc::{Layout};
        use alloc::alloc::{alloc, dealloc};

        if buf.len() < 512 {
            return ioerr!(InvalidInput, "Buffer must be >=512 bytes to read SD sector")
        } else if n > core::i32::MAX as u64 {
            return ioerr!(InvalidInput, "Sector number must be < 2^31 - 1")
        }

        let layout = Layout::from_size_align(buf.len(), 4).expect("Couldn't get layout");
        let buffer = unsafe { alloc(layout) };

        // Read + err in the same unsafe, so we can't have anyone else
        // read & change the error in between. These are on separate
        // lines b/c the order of read THEN check error is important
        // and I'm not sure Rust can guarantee that when constructing
        // a tuple.
        let (result, err) = unsafe {
            LOCK.lock(); // Use mutex guard to auto-unlock
            let read_result = sd_readsector(n as i32, buffer);
            let err_result = sd_err;
            (read_result, err_result)
        };

        let bytes_read = match result {
            0 => match err {
                -1 => ioerr!(TimedOut, "Timeout occured while initializing SD card"),
                -2 => ioerr!(Other, "Unable to send commands to SD card"),
                _ => ioerr!(Other, "Unknown SD card error occured")
            },
            bytes_read => Ok(bytes_read),
        }?;

        let buffer_slice =
            unsafe { core::slice::from_raw_parts(buffer, buf.len()) };
        buf.copy_from_slice(buffer_slice);

        unsafe { dealloc(buffer, layout) };

        Ok(bytes_read as usize)
    }

    fn write_sector(&mut self, _n: u64, _buf: &[u8]) -> io::Result<usize> {
        unimplemented!("SD card and file system are read only")
    }
}
