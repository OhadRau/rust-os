#![feature(asm)]
#![no_std]

use shim::io;

#[cfg(feature = "user-space")]
pub mod syscall;

pub type OsResult<T> = core::result::Result<T, OsError>;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum OsError {
    Unknown = 0,
    Ok = 1,

    NoEntry = 10,
    NoMemory = 20,
    NoVmSpace = 30,
    NoAccess = 40,
    BadAddress = 50,
    FileExists = 60,
    InvalidArgument = 70,

    IoError = 101,
    IoErrorEof = 102,
    IoErrorInvalidData = 103,
    IoErrorInvalidInput = 104,
    IoErrorTimedOut = 105,

    InvalidSocket = 200,
    SocketAlreadyOpen = 201,
    InvalidPort = 202,
}

impl core::convert::From<u64> for OsError {
    fn from(e: u64) -> Self {
        match e {
            1 => OsError::Ok,

            10 => OsError::NoEntry,
            20 => OsError::NoMemory,
            30 => OsError::NoVmSpace,
            40 => OsError::NoAccess,
            50 => OsError::BadAddress,
            60 => OsError::FileExists,
            70 => OsError::InvalidArgument,

            101 => OsError::IoError,
            102 => OsError::IoErrorEof,
            103 => OsError::IoErrorInvalidData,
            104 => OsError::IoErrorInvalidInput,

            200 => OsError::InvalidSocket,
            201 => OsError::SocketAlreadyOpen,
            202 => OsError::InvalidPort,

            _ => OsError::Unknown,
        }
    }
}

impl core::convert::From<io::Error> for OsError {
    fn from(e: io::Error) -> Self {
        match e.kind() {
            io::ErrorKind::UnexpectedEof => OsError::IoErrorEof,
            io::ErrorKind::InvalidData => OsError::IoErrorInvalidData,
            io::ErrorKind::InvalidInput => OsError::IoErrorInvalidInput,
            io::ErrorKind::TimedOut => OsError::IoErrorTimedOut,
            io::ErrorKind::NotFound => OsError::NoEntry,
            _ => OsError::IoError,
        }
    }
}

// Scheduler syscalls
pub const SYS_EXIT: usize = 1;
pub const SYS_SLEEP: usize = 2;
pub const SYS_GETPID: usize = 3;
pub const SYS_FORK: usize = 4;
pub const SYS_EXEC: usize = 5;
pub const SYS_REQUEST_PAGE: usize = 6;

// Miscellaneous I/O syscalls
pub const SYS_TIME: usize = 10;
pub const SYS_INPUT: usize = 11;
pub const SYS_OUTPUT: usize = 12;

// General filesystem syscalls
pub const SYS_FS_CREATE: usize = 20;
pub const SYS_FS_METADATA: usize = 21;
pub const SYS_FS_FLUSH: usize = 22;
pub const SYS_FS_MOUNT: usize = 23;
pub const SYS_FS_UNMOUNT: usize = 24;

// File-specific syscalls
pub const SYS_FILE_OPEN: usize = 30;
pub const SYS_FILE_SEEK: usize = 31;
pub const SYS_FILE_READ: usize = 32;
pub const SYS_FILE_WRITE: usize = 33;
pub const SYS_FILE_CLOSE: usize = 34;
pub const SYS_FILE_DELETE: usize = 35;

// Directory-specific syscalls
pub const SYS_DIR_OPEN: usize = 40;
pub const SYS_DIR_LIST: usize = 41;
pub const SYS_DIR_CLOSE: usize = 42;
pub const SYS_DIR_DELETE: usize = 43;
