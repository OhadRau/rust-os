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
