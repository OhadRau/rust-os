use core::fmt;
use core::fmt::Write;
use core::time::Duration;
//use blockdev::mount::MountOptions;
//extern crate alloc;
//use alloc::boxed::Box;

use crate::*;

pub fn exit() -> ! {
    unsafe { do_syscall0!(SYS_EXIT) }
    loop {}
}

pub fn sleep(span: Duration) -> OsResult<Duration> {
    if span.as_millis() > core::u64::MAX as u128 {
        panic!("too big!");
    }

    let ms = span.as_millis() as u64;

    unsafe { do_syscall1r!(SYS_SLEEP, ms).map(|ms: u64| Duration::from_millis(ms)) }
}

pub fn getpid() -> u64 {
    unsafe { do_syscall1!(SYS_GETPID) }
}

pub fn fork() -> OsResult<u64> {
    unsafe { do_syscall1r!(SYS_FORK) }
}

pub fn exec(path: &str, args: &[&str]) -> OsResult<()> {
    let path_ptr = &path.as_bytes()[0] as *const u8 as u64;
    let path_len = path.len() as u64;

    let args_ptr = args.as_ptr() as u64;
    let args_len = args.len() as u64;
    unsafe { do_syscall0r!(SYS_EXEC, path_ptr, path_len, args_ptr, args_len) }
}

pub fn wait_pid(pid: u64) -> OsResult<()> {
    unsafe { do_syscall0r!(SYS_WAIT_PID, pid) }
}

pub fn request_page(pages: u64) -> OsResult<usize> {
    unsafe { do_syscall1r!(SYS_REQUEST_PAGE, pages).map(|x| x as usize) }
}

pub fn time() -> Duration {
    let (secs, nanos) = unsafe { do_syscall2!(SYS_TIME) };
    Duration::new(secs, nanos as u32)
}

pub fn input() -> u8 {
    unsafe { do_syscall1!(SYS_INPUT) as u8 }
}

pub fn output(b: u8) {
    unsafe { do_syscall0!(SYS_OUTPUT, b as u64) }
}

// Returns amount read
pub fn env_get(var: &str, val: &mut [u8]) -> OsResult<usize> {
    let var_len = var.len() as u64;
    let var_ptr = &var.as_bytes()[0] as *const u8 as u64;

    let val_len = val.len() as u64;

    unsafe { do_syscall1r!(SYS_ENV_GET, var_ptr, var_len, val.as_ptr() as u64, val.len() as u64).map(|x| x as usize) }
}

pub fn env_set(var: &str, val: &str) -> OsResult<()> {
    let var_len = var.len() as u64;
    let var_ptr = &var.as_bytes()[0] as *const u8 as u64;

    let val_len = val.len() as u64;
    let val_ptr = &val.as_bytes()[0] as *const u8 as u64;

    unsafe { do_syscall0r!(SYS_ENV_SET, var_ptr, var_len, val_ptr, val_len) }
}

pub fn fs_create(path: &str, kind: EntryKind) -> OsResult<()> {
    let path_ptr = &path.as_bytes()[0] as *const u8 as u64;
    let path_len = path.len() as u64;

    unsafe { do_syscall0r!(SYS_FS_CREATE, path_ptr, path_len, kind.as_u64()) }
}

pub fn fs_open(path: &str) -> OsResult<Fd> {
    let path_ptr = &path.as_bytes()[0] as *const u8 as u64;
    let path_len = path.len() as u64;

    unsafe {
        do_syscall1r!(SYS_FS_OPEN, path_ptr, path_len).map(Fd::from)
    }
}

pub fn fs_close(fd: &Fd) -> OsResult<()> {
    unsafe { do_syscall0r!(SYS_FS_CLOSE, fd.as_u64()) }
}

pub fn fs_delete(path: &str) -> OsResult<()> {
    let path_ptr = &path.as_bytes()[0] as *const u8 as u64;
    let path_len = path.len() as u64;

    unsafe { do_syscall0r!(SYS_FS_DELETE, path_ptr, path_len) }
}

pub fn file_seek(fd: &Fd, sf: shim::io::SeekFrom) -> OsResult<u64> {
    let (mode, offset) = seek_mode_to_raw(sf);
    unsafe { do_syscall1r!(SYS_FILE_SEEK, fd.as_u64(), mode, offset as u64) }
}

pub fn file_read(fd: &Fd, buf: &mut [u8]) -> OsResult<usize> {
    unsafe {
        do_syscall1r!(SYS_FILE_READ, fd.as_u64(), buf.as_mut_ptr() as u64, buf.len() as u64)
            .map(|x| x as usize)
    }
}

pub fn mount(part_num: u64, path: &str, encrypted: bool) -> OsResult<()> {
    let path_ptr = &path.as_bytes()[0] as *const u8 as u64;
    let path_len = path.len() as u64;

    unsafe { do_syscall0r!(SYS_FS_MOUNT, part_num, path_ptr, path_len, encrypted as u64) }
}

pub fn unmount(path: &str) -> OsResult<()> {
    let path_ptr = &path.as_bytes()[0] as *const u8 as u64;
    let path_len = path.len() as u64;

    unsafe { do_syscall0r!(SYS_FS_UNMOUNT, path_ptr, path_len) }

}

pub fn lsblk() {
    // this should prob return a cloned mount map but I'm lazy
    unsafe { do_syscall0!(SYS_FS_LSBLK) }
}

// returns true if there are remaining directory entries
// returns false if no remaining entries
// returns error otherwise
pub fn dir_entry(path: &str, entry_buf: &mut [u8], offset: usize) -> OsResult<bool> {
    let path_ptr = path.as_ptr() as u64;
    let path_len = path.len() as u64;
    let entry_buf_ptr = entry_buf.as_ptr() as u64;
    let entry_buf_len = entry_buf.len() as u64;
    
    unsafe { do_syscall1r!(SYS_DIR_ENTRY, path_ptr, path_len, entry_buf_ptr, entry_buf_len, offset as u64).map(|x| x != 0) }
}

struct Console;

impl fmt::Write for Console {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            output(b);
        }
        Ok(())
    }
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::syscall::vprint(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
 () => (print!("\r\n"));
    ($($arg:tt)*) => ({
        $crate::syscall::vprint(format_args!($($arg)*));
        $crate::print!("\r\n");
    })
}

pub fn vprint(args: fmt::Arguments) {
    let mut c = Console;
    c.write_fmt(args).unwrap();
}
