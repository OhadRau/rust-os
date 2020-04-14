use hashbrown::HashMap;
use fat32::vfat::{VFat, VFatHandle, Error};
use fat32::traits::{BlockDevice, FileSystem};
use fat32::mbr::MasterBootRecord;
use alloc::boxed::Box;
use alloc::vec::Vec;
use shim::path::{PathBuf, Path};
use crate::fs::PiVFatHandle;
use crate::console::kprintln;

/*pub struct MapEntry<'a, HANDLE: VFatHandle> {
    vfat: VFat<HANDLE>
}*/
pub struct MountMap { 
    map: HashMap<PathBuf, Box<PiVFatHandle>>
}

impl MountMap {
    pub fn new() -> MountMap {
        MountMap {
            map: HashMap::new()
        }
    }

    /// mount partition number part_num to mount_point 
    /// pointed to by mount_point. Initializes the VFat
    /// for that partition and inserts it into thee map.
    pub fn mount<T>(&mut self, mount_point: PathBuf, mut device: T, part_num: usize) -> Result<(), Error>
    // maybe not use static lifetime? vfat uses it
    where T: BlockDevice + 'static, {
        let vfat = match VFat::<PiVFatHandle>::from(device, part_num) {
            Ok(handle) => handle,
            Err(e) => return Err(e)
        };

        self.map.insert(mount_point, Box::new(vfat));
        Ok(())
    }

    /// unmounts the filesystem pointed to by mount_point
    /// flushes the filesystem and then drops it
    pub fn unmount(&mut self, mount_point: PathBuf) {
        match self.map.remove(&mount_point) {
            Some(vfat) => vfat.flush(),
            None => return
        };
    }

    /*pub fn unmount_all(&mut self) {
        for key in self.keys() {
            self.unmount(key);
        }
    }*/

    /// Takes a path and returns the filesystem that's mounted there
    pub fn route(&mut self, path: PathBuf) -> Result<&mut PiVFatHandle, ()> {
        //kprintln!("routing: {}", path.to_str().unwrap());
        // we first get a vector of all the mount points that are prefixes of path
        let candidates: Vec<PathBuf> = self.map.keys().filter(|mount_point| path.starts_with(mount_point)).map(|pb| pb.clone()).collect();
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
            
        match mounted_at {
            // unwrap here should be safe because we know for a fact that
            // p is a key of the map
            Some(p) => Ok(self.map.get_mut(p).unwrap()),
            None => Err(())
        }
    }
}