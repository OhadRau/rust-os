use hashbrown::HashMap;
use fat32::vfat::{VFat, VFatHandle, Error};
use fat32::traits::{BlockDevice, FileSystem};
use fat32::mbr::MasterBootRecord;
use alloc::boxed::Box;
use alloc::vec::Vec;
use shim::path::{PathBuf, Path};
use shim::{ioerr, io};
use crate::fs::PiVFatHandle;
use crate::console::kprintln;
use core::fmt;

pub struct MapEntry {
    vfat: PiVFatHandle,
    part_num: usize,
}

pub struct MountMap { 
    map: HashMap<PathBuf, Box<MapEntry>>
}

impl MountMap {
    pub fn new() -> MountMap {
        MountMap {
            map: HashMap::new()
        }
    }

    // we need to mount the root partition before mounting any other ones because all the mountpoints
    // need exist as directories in the FS mounted as /
    pub fn mount_root<T>(&mut self, mut device: T, part_num: usize) -> io::Result<()>
    where T: BlockDevice + 'static, {
        self.do_mount(&PathBuf::from("/"), device, part_num)
    }

    /// mount partition number part_num to mount_point 
    /// pointed to by mount_point. Initializes the VFat
    /// for that partition and inserts it into thee map.
    pub fn mount<T>(&mut self, mount_point: &PathBuf, mut device: T, part_num: usize) -> io::Result<()>
    // maybe not use static lifetime? vfat uses it
    where T: BlockDevice + 'static, {
        // check that the mount point exists
        match self.route(mount_point) {
            Ok((vfat, real_mount_path)) => match vfat.open_dir(real_mount_path) {
                Ok(dir) => (),
                Err(_) => return ioerr!(NotFound, "Unable to mount device: mount point does not exist")
            },
            Err(_) => return ioerr!(NotFound, "Unable to mount device: mount point does not exist")
        }
        
        // mount it
        self.do_mount(mount_point, device, part_num)
    }

    fn do_mount<T>(&mut self, mount_point: &PathBuf, mut device: T, part_num: usize) -> io::Result<()> 
    where T: BlockDevice + 'static, {
        if self.map.contains_key(&mount_point.clone()) {
            return ioerr!(InvalidData, "mount point already mounted!!");
        }

        let vfat = match VFat::<PiVFatHandle>::from(device, part_num) {
            Ok(handle) => handle,
            Err(e) => return ioerr!(InvalidData, "Error intiailizing filesystem")
        };

        self.map.insert(mount_point.clone(), Box::new(MapEntry { vfat, part_num }));
        Ok(())

    }

    /// unmounts the filesystem pointed to by mount_point
    /// flushes the filesystem and then drops it
    pub fn unmount(&mut self, mount_point: &PathBuf) {
        match self.map.remove(mount_point) {
            Some(entry) => entry.vfat.flush(),
            None => return
        };
    }

    /*pub fn unmount_all(&mut self) {
        for key in self.keys() {
            self.unmount(key);
        }
    }*/

    /// Takes a path and returns the filesystem that's mounted there,
    /// along with the translated path 
    /// for example if the mount point is /boot and the path is /boot
    /// the real path is /, when we're using the filesystem mounted at /boot
    pub fn route(&mut self, path: &PathBuf) -> Result<(&mut PiVFatHandle, PathBuf), ()> {
        kprintln!("routing: {}", path.to_str().unwrap());
        // we first get a vector of all the mount points that are prefixes of path
        let candidates: Vec<PathBuf> = self.map.keys().filter(|mount_point| path.starts_with(mount_point)).map(|pb| pb.clone()).collect();
        kprintln!("candidates {:?}", candidates);
        // then we get the longest one, which should be the real mount point
        let mounted_at = candidates.iter().fold(None, |longest: Option<&PathBuf>, candidate: &PathBuf| {
                let cand_len = match candidate.to_str() {
                    Some(s) => s.len(),
                    None => 0
                };

                let longest_len = match longest {
                    Some(l) => match l.to_str() {
                        Some(l_str) => l_str.len(),
                        None => 0
                    },
                    None => 0
                };

                if cand_len > longest_len {
                    Some(candidate)
                } else {
                    longest
                }
            });
        kprintln!("mounted at: {:?}", mounted_at);
        match mounted_at {
            // unwrap here should be safe because we know for a fact that
            // p is a key of the map
            Some(p) => {
                Ok((&mut self.map.get_mut(p).unwrap().vfat, Self::translate_path(path, p.into())))
            },
            None => Err(())
        }
    }

    fn translate_path<P: AsRef<Path>>(path: P, mount_point: P) -> PathBuf {
        // this should never fail because mount_point should always be a
        // prefix of the path we want to translate
        // if it fails, it was probably called incorrectly
        (*path.as_ref().strip_prefix(mount_point.as_ref()).unwrap()).to_path_buf()
    }
}

impl fmt::Display for MountMap {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for (path, map_entry) in self.map.iter() {
            write!(f, "{} -> {}\n", map_entry.part_num, path.as_path().to_str().unwrap())?;
        }
        Ok(())
    }
}