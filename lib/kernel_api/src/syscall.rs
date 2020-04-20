use core::fmt;
use core::fmt::Write;
use core::time::Duration;

use crate::*;
use crate::syscall_macros::*;

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
pub fn env_get(var: &str, val: &mut str) -> OsResult<usize> {
    let var_len = var.len() as u64;
    let var_ptr = &var.as_bytes()[0] as *const u8 as u64;

    let val_len = val.len() as u64;
    let val_ptr = unsafe { &mut val.as_bytes_mut()[0] as *mut u8 as u64 };

    unsafe { do_syscall1r!(SYS_ENV_GET, var_ptr, var_len, val_ptr, val_len).map(|x| x as usize) }
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
