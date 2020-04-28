use alloc::boxed::Box;
use alloc::string::String;
use shim::path::PathBuf;

use crate::console::CONSOLE;
use crate::process::State;
use crate::traps::TrapFrame;
use crate::SCHEDULER;
use kernel_api::*;
use crate::fs::fd::Fd;

/// Kills current process.
///
/// This system call does not take parameter and does not return any value.
pub fn sys_exit(tf: &mut TrapFrame) {
    let _ = SCHEDULER.kill(tf);
}

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

/// Forks a process into two separate processes.
///
/// This system call does not take parameters.
///
/// In addition to the usual status value, this system call returns
/// a parameter: 0 for the child process and the child's PID for the
/// parent process.
pub fn sys_fork(tf: &mut TrapFrame) {
    tf.xs[0] = 0; // Set the first result to 0 before forking so the child sees a 0
    tf.xs[7] = 1; // Success
    match SCHEDULER.fork(tf) {
        Some(new_pid) => {
            tf.xs[0] = new_pid;
        },
        None => {
            tf.xs[7] = 0; // Unknown error
        }
    } 
}

pub fn sys_exec(program: *const u8, program_len: usize, args: *const &str, args_len: usize, tf: &mut TrapFrame){
    let string = unsafe { core::slice::from_raw_parts(program, program_len) };
    let path = match core::str::from_utf8(string) {
        Ok(name) => name,
        Err(_) => {
            tf.xs[7] = 0; // Unknown error
            return
        }
    };
    let args: &[&str] = unsafe { core::slice::from_raw_parts(args, args_len) };

    let result = SCHEDULER.with_running(|process: &mut crate::process::Process| {
        match process.load_existing(path) {
            Ok(()) => {
                process.init_args(args);
                *tf = *process.context;
            },
            Err(_) => tf.xs[7] = 0, // Unknown error
        }
    });
    match result {
        Some(_) => (),
        None => tf.xs[7] = 0, // Unknown error
    }
}

pub fn sys_wait_pid(pid: u64, tf: &mut TrapFrame) {
    use core::sync::atomic::Ordering;

    match SCHEDULER.get_dead_handle(pid as crate::process::Id) {
        Some(dead) => {
            tf.xs[7] = 1; // Success
            let is_ready = Box::new(move |_: &mut crate::process::Process| {
                dead.load(Ordering::Relaxed)
            });

            SCHEDULER.switch(State::Waiting(is_ready), tf);
        },
        None => {
            tf.xs[7] = 0; // Unknown error
        }
    }
}

pub fn sys_request_page(num_pages: u64, tf: &mut TrapFrame) {
    use crate::param::PAGE_SIZE;
    use crate::vm::{PagePerm, VirtualAddr};

    SCHEDULER.with_running(|process| {
        // Return the PREVIOUS program break
        tf.xs[0] = process.last_page.as_u64() + (PAGE_SIZE as u64);
        for _ in 0..num_pages {
            let base_addr = process.last_page + VirtualAddr::from(PAGE_SIZE);
            process.vmap.alloc(VirtualAddr::from(base_addr), PagePerm::RWX);
            process.last_page = base_addr;
        }
        tf.xs[7] = 1; // Success
    });
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

/// Read from console.
///
/// This system call does not take parameter.
///
/// In addition to the usual status value, this system call returns one
/// parameters:
///  - the read character
pub fn sys_input(tf: &mut TrapFrame) {
    tf.xs[0] = CONSOLE.lock().read_byte() as u64;
    tf.xs[7] = 1; // success
}

/// Write to console.
///
/// This system call takes one parameter: a u8 character to print.
///
/// It only returns the usual status value.
pub fn sys_output(b: u8, tf: &mut TrapFrame) {
    CONSOLE.lock().write_byte(b);
    tf.xs[7] = 1; // success
}

pub fn sys_env_get(var_ptr: *const u8, var_len: usize, val_ptr: *mut u8, val_len: usize, tf: &mut TrapFrame) {
    let var_slice = unsafe { core::slice::from_raw_parts(var_ptr, var_len) };
    let var_string = match core::str::from_utf8(var_slice) {
        Ok(var_string) => var_string,
        Err(_) => {
            tf.xs[7] = 70; // Invalid argument
            return
        }
    };

    let val_buf = unsafe { core::slice::from_raw_parts_mut(val_ptr, val_len) };

    match SCHEDULER.with_running(|process| -> Option<String> {
        process.env.get(var_string).map(String::from)
    }).and_then(|x| x) {
        Some(string) => {
            let length = core::cmp::min(string.len(), val_buf.len());
            if length == string.len() {
                val_buf[0..length].copy_from_slice(&string.as_bytes()[0..length]);
                tf.xs[0] = length as u64; // length bytes read
                tf.xs[7] = 1; // Success
            } else {
                tf.xs[7] = 20; // No memory
            }
        },
        None => {
            tf.xs[7] = 10; // No entry
        }
    }
}

pub fn sys_env_set(var_ptr: *const u8, var_len: usize, val_ptr: *const u8, val_len: usize, tf: &mut TrapFrame) {
    let var_slice = unsafe { core::slice::from_raw_parts(var_ptr, var_len) };
    let var_string = match core::str::from_utf8(var_slice) {
        Ok(var_string) => var_string,
        Err(_) => {
            tf.xs[7] = 70; // Invalid argument
            tf.xs[0] = 0; // 0 bytes read
            return
        }
    };

    let val_slice = unsafe { core::slice::from_raw_parts(val_ptr, val_len) };
    let val_string = match core::str::from_utf8(val_slice) {
        Ok(val_string) => val_string,
        Err(_) => {
            tf.xs[7] = 70; // Invalid argument
            tf.xs[0] = 0; // 0 bytes read
            return
        }
    };

    match SCHEDULER.with_running(|process| {
        let _ = process.env.insert(String::from(var_string), String::from(val_string));
    }) {
        Some(_) => tf.xs[7] = 1, // Success
        None => tf.xs[7] = 0, // Unknown error
    }
}

fn parse_path(path_ptr: *const u8, path_len: usize) -> Option<PathBuf> {
    use shim::path::Component;
    fn canonicalize(path: PathBuf) -> Option<PathBuf> {
        let mut new_path = PathBuf::new();
        for comp in path.components() {
            match comp {
                Component::ParentDir => {
                    let res = new_path.pop();
                    if !res {
                        return None;
                    }
                },
                Component::Normal(n) => new_path = new_path.join(n),
                Component::RootDir => new_path = ["/"].iter().collect(),
                _ => ()
            };
        }
        Some(new_path)
    }

    let path_slice = unsafe { core::slice::from_raw_parts(path_ptr, path_len) };
    let path_string = core::str::from_utf8(path_slice).ok()?;
    let raw_path = PathBuf::from(path_string);
    if !raw_path.is_absolute() { return None }
    canonicalize(raw_path)
}

pub fn sys_fs_create(path_ptr: *const u8, path_len: usize, kind: EntryKind, tf: &mut TrapFrame) {
    use fat32::traits::{Dir, Entry};
    use shim::{io, ioerr};

    let path = match parse_path(path_ptr, path_len) {
        Some(path) => path,
        None => {
            tf.xs[7] = 70; // Invalid argument
            return
        },
    };

    let parent = path.parent().unwrap_or(&PathBuf::from("/")).to_path_buf();
    let child = path.file_name().expect("Must provide filename to create")
                    .to_str().expect("Could not convert filename to string");

    // TODO: Verify that the name is valid

    let attributes = match kind {
        EntryKind::File => fat32::vfat::Attributes::default(),
        EntryKind::Dir => fat32::vfat::Attributes::default().dir(),
    };

    let err = SCHEDULER.with_running(|process: &mut crate::process::Process| -> io::Result<()> {
        let parent_fd = process.fd_table.open(parent)?;
        process.fd_table.critical(&parent_fd, move |entry| -> io::Result<()> {
            let dir = match entry.as_dir_mut() {
                Some(dir) => dir,
                None => return ioerr!(InvalidInput, ""),
            };

            // Don't create a duplicate if it already exists
            if let Ok(found) = dir.find(child) {
                match (kind, found.is_dir()) {
                    (EntryKind::Dir, true) => return Ok(()),
                    (EntryKind::File, false) => return Ok(()),
                    (_, _) => return ioerr!(AlreadyExists, ""),
                }
            }

            dir.create(fat32::vfat::Metadata {
                name: String::from(child),
                attributes,
                ..Default::default()
            }).map(|_| ())
        }).and_then(|x| x)?;
        process.fd_table.close(&parent_fd)
    });

    // TODO: Actually interpret the IO errors
    match err {
        Some(Ok(_)) => tf.xs[7] = 1, // Success
        _ => tf.xs[7] = 0, // Unknown error
    }
}

pub fn sys_fs_open(path_ptr: *const u8, path_len: usize, tf: &mut TrapFrame) {
    let path = match parse_path(path_ptr, path_len) {
        Some(path) => path,
        None => {
            tf.xs[7] = 70; // Invalid argument
            return
        },
    };

    SCHEDULER.with_running(|process: &mut crate::process::Process| {
        match process.fd_table.open(path) {
            Ok(fd) => {
                tf.xs[0] = fd.as_u64();
                tf.xs[7] = 1; // Success
            },
            Err(_) => tf.xs[7] = 0, // Unknown error
        }
    });
}

pub fn sys_fs_close(fd: Fd, tf: &mut TrapFrame) {
    SCHEDULER.with_running(|process: &mut crate::process::Process| {
        match process.fd_table.close(&fd) {
            Ok(_) => tf.xs[7] = 1, // Success
            Err(_) => tf.xs[7] = 0, // Unknown error
        }
    });
}

pub fn sys_fs_delete(path_ptr: *const u8, path_len: usize, tf: &mut TrapFrame) {
    use shim::io;
    use fat32::traits::{Dir, Entry, File};

    let path = match parse_path(path_ptr, path_len) {
        Some(path) => path,
        None => {
            tf.xs[7] = 70; // Invalid argument
            return
        },
    };

    let err = SCHEDULER.with_running(|process| {
        let fd = process.fd_table.open(path)?;
        process.fd_table.critical(&fd, move |entry| -> io::Result<()> {
            if entry.is_file() {
                entry.as_file_mut().expect("Unable to open file as file").delete()
            } else {
                entry.as_dir_mut().expect("Unable to open dir as dir").delete()
            }
        }).and_then(|x| x)?;
        process.fd_table.close(&fd)
    });

    match err {
        Some(Ok(_)) => tf.xs[7] = 1, // Success
        Some(Err(e)) => match e.kind() {
            io::ErrorKind::NotFound         => tf.xs[7] = 10,  // No entry
            io::ErrorKind::PermissionDenied => tf.xs[7] = 40,  // No access
            io::ErrorKind::AddrInUse        => tf.xs[7] = 201, // Already open
            _                               => tf.xs[7] = 0,   // Unknown
        },
        None => tf.xs[7] = 0, // Unknown
    }
}

pub fn sys_file_seek(fd: Fd, mode: u64, offset: i64, tf: &mut TrapFrame) {
    use shim::{io, ioerr};
    use io::Seek;
    use fat32::traits::Entry;

    let sf = seek_mode_from_raw(mode, offset);
    let err = SCHEDULER.with_running(|process| {
        process.fd_table.critical(&fd, move |entry| -> io::Result<u64> {
            if entry.is_dir() { return ioerr!(InvalidInput, "Can't seek in a directory") }
            entry.as_file_mut().expect("Unable to open file as file").seek(sf)
        }).and_then(|x| x)
    });

    match err {
        Some(Ok(n)) => {
            tf.xs[0] = n;
            tf.xs[7] = 1; // Success
        },
        _ => tf.xs[7] = 0, // Unknown
    }
}

pub fn sys_file_read(fd: Fd, buf: *mut u8, buf_len: usize, tf: &mut TrapFrame) {
    use shim::{io, ioerr};
    use io::Read;
    use fat32::traits::Entry;

    let buf_slice = unsafe { core::slice::from_raw_parts_mut(buf, buf_len) };
    let err = SCHEDULER.with_running(|process| {
        process.fd_table.critical(&fd, move |entry| -> io::Result<usize> {
            if entry.is_dir() { return ioerr!(InvalidInput, "Can't seek in a directory") }
            entry.as_file_mut().expect("Unable to open file as file").read(buf_slice)
        }).and_then(|x| x)
    });

    match err {
        Some(Ok(n)) => {
            tf.xs[0] = n as u64;
            tf.xs[7] = 1; // Success
        },
        _ => tf.xs[7] = 0, // Unknown
    }
}

pub fn handle_syscall(num: u16, tf: &mut TrapFrame) {
    match num as usize {
        SYS_EXIT => sys_exit(tf),
        SYS_SLEEP => sys_sleep(tf.xs[0] as u32, tf),
        SYS_GETPID => sys_getpid(tf),
        SYS_FORK => sys_fork(tf),
        SYS_EXEC => sys_exec(tf.xs[0] as *const u8, tf.xs[1] as usize, tf.xs[2] as *const &str, tf.xs[3] as usize, tf),
        SYS_WAIT_PID => sys_wait_pid(tf.xs[0], tf),
        SYS_REQUEST_PAGE => sys_request_page(tf.xs[0], tf),

        SYS_TIME => sys_time(tf),
        SYS_INPUT => sys_input(tf),
        SYS_OUTPUT => sys_output(tf.xs[0] as u8, tf),
        SYS_ENV_GET => sys_env_get(tf.xs[0] as *const u8, tf.xs[1] as usize, tf.xs[2] as *mut u8, tf.xs[3] as usize, tf),
        SYS_ENV_SET => sys_env_set(tf.xs[0] as *const u8, tf.xs[1] as usize, tf.xs[2] as *const u8, tf.xs[3] as usize, tf),

        SYS_FS_CREATE => sys_fs_create(tf.xs[0] as *const u8, tf.xs[1] as usize, EntryKind::from(tf.xs[2]), tf),
        SYS_FS_OPEN => sys_fs_open(tf.xs[0] as *const u8, tf.xs[1] as usize, tf),
        SYS_FS_CLOSE => sys_fs_close(Fd::from(tf.xs[0]), tf),
        SYS_FS_DELETE => sys_fs_delete(tf.xs[0] as *const u8, tf.xs[1] as usize, tf),

        SYS_FILE_SEEK => sys_file_seek(Fd::from(tf.xs[0]), tf.xs[1], tf.xs[2] as i64, tf),
        SYS_FILE_READ => sys_file_read(Fd::from(tf.xs[0]), tf.xs[1] as *mut u8, tf.xs[2] as usize, tf),

        _ => {
            tf.xs[7] = OsError::Unknown as u64;
        }
    }
}
