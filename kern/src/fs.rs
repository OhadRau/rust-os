pub mod sd;
pub mod mount_map;

use alloc::rc::Rc;
use core::fmt::{self, Debug};
use shim::io;
use shim::ioerr;
use shim::path::{Path, PathBuf};

pub use fat32::traits;
use fat32::vfat::{Dir, Entry, File, VFat, VFatHandle};

use self::sd::Sd;
use self::mount_map::MountMap;
use crate::mutex::Mutex;
use crate::console::kprintln;

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
pub struct FileSystem(Mutex<Option<MountMap>>);
/// plan: make a MountMap struct that maps a directory to a VFat 
/// Make this FileSystem struct a wrapper around MountMap option
/// MountMap will have mount/unmount methods that will associate it with Directories
/// This FileSystem impl will handle routing of requests to appropriate VFS based on the path
/// use path.starts_with
impl FileSystem {
    /// Returns an uninitialized `FileSystem`.
    ///
    /// The file system must be initialized by calling `initialize()` before the
    /// first memory allocation. Failure to do will result in panics.
    pub const fn uninitialized() -> Self {
        FileSystem(Mutex::new(None))
    }

    /// Initializes the file system.
    /// The caller should assure that the method is invoked only once during the
    /// kernel initialization.
    ///
    /// # Panics
    ///
    /// Panics if the underlying disk or file sytem failed to initialize.
    pub unsafe fn initialize(&self) {
        let mut guard = self.0.lock();

        match *guard {
            Some(_) => panic!("Attempted to initialize FS twice"),
            None => (),
        }
        let mut mount_map = MountMap::new();
        let sd = Sd::new().expect("Unable to init SD card");
        match mount_map.mount_root(sd, 2) {
            Ok(_) => (),
            Err(_) => panic!("unable to mount root filesystem")
        }
        //let fs = VFat::<PiVFatHandle>::from(sd, 1).expect("Unable to init VFat");
        /*match mount_map.mount(&PathBuf::from("/boot"), sd, 1) {
            Err(e) => kprintln!("{:?}", e),
            Ok(_) => ()
        }*/
        *guard = Some(mount_map);
    }

    pub fn flush_fs<P: AsRef<Path>>(&self, path: P) {
        use fat32::traits::FileSystem;
        let mut map = self.0.lock();
        match &mut *map {
            Some(map) => match map.route(&path.as_ref().to_path_buf()) {
                Ok((vfat, real_path)) => vfat.flush(),
                Err(_) => () // add error reporting??
            },
            None => (),
        }
    }

    pub fn lsblk(&self) {
        match &*self.0.lock() {
            Some(map) => kprintln!("{}", map),
            None => kprintln!("no map exists")
        }
    }

    pub fn mount(&self, part_num: usize, mount_point: PathBuf) {
        match &mut *self.0.lock() {
            // passing in a blank Sd struct should work because the 
            // sd descriptor is stored statically in the sd driver
            // this assumes that sd driver has been initialized prior to this call
            Some(map) => match map.mount(&mount_point, Sd {}, part_num) {
                Ok(_) => kprintln!("mount successful"),
                Err(e) => kprintln!("mount failed: {:?}", e)
            },
            None => kprintln!("no map exists")
        }
    }

    pub fn unmount(&self, mount_point: PathBuf) {
        match &mut *self.0.lock() {
            Some(map) => map.unmount(&mount_point),
            None => kprintln!("no map exists")
        }
    }
}

// Implement `fat32::traits::FileSystem` for `&FileSystem`
impl fat32::traits::FileSystem for &FileSystem {
    /// The type of files in this file system.
    type File = File<PiVFatHandle>;

    /// The type of directories in this file system.
    type Dir = Dir<PiVFatHandle>;

    /// The type of directory entries in this file system.
    type Entry = Entry<PiVFatHandle>;
    type Metadata = fat32::vfat::Metadata;

    /// Opens the entry at `path`. `path` must be absolute.
    ///
    /// # Errors
    ///
    /// If `path` is not absolute, an error kind of `InvalidInput` is returned.
    ///
    /// If any component but the last in `path` does not refer to an existing
    /// directory, an error kind of `InvalidInput` is returned.
    ///
    /// If there is no entry at `path`, an error kind of `NotFound` is returned.
    ///
    /// All other error values are implementation defined.
    fn open<P: AsRef<Path>>(self, path: P) -> io::Result<Self::Entry> {
        let mut map = self.0.lock();
        match &mut *map {
            Some(map) => match map.route(&path.as_ref().to_path_buf()) {
                Ok((vfat, real_path)) => vfat.open(real_path),
                Err(_) => {
                    ioerr!(NotFound, "Path is not mounted") 
                }
            },
            None => ioerr!(Other, "Filesystem must be initialized before calling open()"),
        }
        //unimplemented!()
    }

    fn flush(self) {
        /*let mut fs = self.0.lock();
        match &*fs {
            Some(fs) => fs.flush(),
            None => (),
        }*/
        /*let map = self.0.lock();
        match &mut *map {
            Some(map) => map.unmount_all(),
            None => ioerr!(Other, "Filesystem must be initialized before calling flush()"),
        }*/
        unimplemented!("thou shall not flush the FileSystem container!!")
    }
}
