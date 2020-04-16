use alloc::boxed::Box;

use crate::console::CONSOLE;
use crate::process::State;
use crate::traps::TrapFrame;
use crate::SCHEDULER;
use kernel_api::*;

/// Sleep for `ms` milliseconds.
///
/// This system call takes one parameter: the number of milliseconds to sleep.
///
/// In addition to the usual status value, this system call returns one
/// parameter: the approximate true elapsed time from when `sleep` was called to
/// when `sleep` returned.
pub fn sys_sleep(ms: u32, tf: &mut TrapFrame) {
    use pi::timer;
    let start_time = timer::current_time().as_millis() as u64;

    let is_ready = Box::new(move |p: &mut crate::process::Process| {
        let now = timer::current_time().as_millis() as u64;
        let diff = now - start_time;
        p.context.xs[0] = diff;
        p.context.xs[7] = 1;
        diff >= ms as u64
    });

    SCHEDULER.switch(State::Waiting(is_ready), tf);
}

/// Returns current time.
///
/// This system call does not take parameter.
///
/// In addition to the usual status value, this system call returns two
/// parameter:
///  - current time as seconds
///  - fractional part of the current time, in nanoseconds.
pub fn sys_time(tf: &mut TrapFrame) {
    let time = pi::timer::current_time();
    let secs = time.as_secs();
    let nanos = time.subsec_nanos();

    tf.xs[0] = secs;
    tf.xs[1] = nanos as u64;
    tf.xs[7] = 1; // success
}

/// Kills current process.
///
/// This system call does not take parameter and does not return any value.
pub fn sys_exit(tf: &mut TrapFrame) {
    let _ = SCHEDULER.kill(tf);
}

/// Write to console.
///
/// This system call takes one parameter: a u8 character to print.
///
/// It only returns the usual status value.
pub fn sys_write(b: u8, tf: &mut TrapFrame) {
    CONSOLE.lock().write_byte(b);
    tf.xs[7] = 1; // success
}

/// Returns current process's ID.
///
/// This system call does not take parameter.
///
/// In addition to the usual status value, this system call returns a
/// parameter: the current process's ID.
pub fn sys_getpid(tf: &mut TrapFrame) {
    tf.xs[0] = tf.tpidr; // return PID from trapframe
    tf.xs[7] = 1; // success
}

pub fn handle_syscall(num: u16, tf: &mut TrapFrame) {
    match num as usize {
        NR_SLEEP => sys_sleep(tf.xs[0] as u32, tf),
        NR_TIME => sys_time(tf),
        NR_EXIT => sys_exit(tf),
        NR_WRITE => sys_write(tf.xs[0] as u8, tf),
        NR_GETPID => sys_getpid(tf),
        _ => {
            tf.xs[7] = OsError::Unknown as u64;
        }
    }
}
