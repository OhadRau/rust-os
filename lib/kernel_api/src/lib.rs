#![feature(asm)]
#![no_std]

use shim::io;
use shim::io::SeekFrom;

#[cfg(feature = "user-space")]
pub mod syscall;
mod syscall_macros;

pub type OsResult<T> = core::result::Result<T, OsError>;

pub const ARG_MAX: usize = 32;

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

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct Fd(u64);

impl Fd {
  pub fn as_u64(&self) -> u64 {
    self.0
  }
}

impl core::convert::From<u64> for Fd {
  fn from(d: u64) -> Self {
    Fd(d)
  }
}

#[derive(Debug, PartialEq, Eq)]
pub enum EntryKind { File, Dir }

impl EntryKind {
  pub fn as_u64(&self) -> u64 {
    match self {
      EntryKind::File => 0,
      EntryKind::Dir => 1,
    }
  }
}

impl core::convert::From<u64> for EntryKind {
  fn from(e: u64) -> Self {
    match e {
      0 => EntryKind::File,
      1 => EntryKind::Dir,
      _ => panic!("Unknown EntryKind"),
    }
  }
}

pub const SEEK_FROM_START: u64 = 0;
pub const SEEK_FROM_CURRENT: u64 = 1;
pub const SEEK_FROM_END: u64 = 2;

pub fn seek_mode_to_raw(sf: SeekFrom) -> (u64, i64) {
  match sf {
    SeekFrom::Start(n) => (SEEK_FROM_START, n as i64),
    SeekFrom::End(n) => (SEEK_FROM_END, n),
    SeekFrom::Current(n) => (SEEK_FROM_CURRENT, n),
  }
}

pub fn seek_mode_from_raw(mode: u64, offset: i64) -> SeekFrom {
  if mode == SEEK_FROM_START {
    SeekFrom::Start(offset as u64)
  } else if mode == SEEK_FROM_END {
    SeekFrom::End(offset)
  } else if mode == SEEK_FROM_CURRENT {
    SeekFrom::Current(offset)
  } else {
    panic!("Encountered invalid seek mode")
  }
}

// Scheduler syscalls
pub const SYS_EXIT: usize = 1;
pub const SYS_SLEEP: usize = 2;
pub const SYS_GETPID: usize = 3;
pub const SYS_FORK: usize = 4;
pub const SYS_EXEC: usize = 5;
pub const SYS_WAIT_PID: usize = 6;
pub const SYS_REQUEST_PAGE: usize = 7;

// Miscellaneous I/O syscalls
pub const SYS_TIME: usize = 10;
pub const SYS_INPUT: usize = 11;
pub const SYS_OUTPUT: usize = 12;
pub const SYS_ENV_GET: usize = 13;
pub const SYS_ENV_SET: usize = 14;
pub const SYS_ENV_VARS: usize = 15;

// General filesystem syscalls
pub const SYS_FS_CREATE: usize = 20;
pub const SYS_FS_OPEN: usize = 21;
pub const SYS_FS_CLOSE: usize = 22;
pub const SYS_FS_DELETE: usize = 23;
pub const SYS_FS_METADATA: usize = 24;
pub const SYS_FS_FLUSH: usize = 25;
pub const SYS_FS_MOUNT: usize = 26;
pub const SYS_FS_UNMOUNT: usize = 27;

// File-specific syscalls
pub const SYS_FILE_SEEK: usize = 30;
pub const SYS_FILE_READ: usize = 31;
pub const SYS_FILE_WRITE: usize = 32;

// Directory-specific syscalls
pub const SYS_DIR_LIST: usize = 40;
