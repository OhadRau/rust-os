use alloc::boxed::Box;
use alloc::vec::Vec;
use core::fmt;
use hashbrown::HashMap;
use shim::io;
use shim::ioerr;

use crate::traits::BlockDevice;

#[derive(Debug)]
struct CacheEntry {
    data: Vec<u8>,
    dirty: bool,
}

pub struct Partition {
    /// The physical sector where the partition begins.
    pub start: u64,
    /// Number of sectors
    pub num_sectors: u64,
    /// The size, in bytes, of a logical sector in the partition.
    pub sector_size: u64,
}

pub struct CachedPartition {
    device: Box<dyn BlockDevice>,
    cache: HashMap<u64, CacheEntry>,
    partition: Partition,
}

impl CachedPartition {
    /// Creates a new `CachedPartition` that transparently caches sectors from
    /// `device` and maps physical sectors to logical sectors inside of
    /// `partition`. All reads and writes from `CacheDevice` are performed on
    /// in-memory caches.
    ///
    /// The `partition` parameter determines the size of a logical sector and
    /// where logical sectors begin. An access to a sector `0` will be
    /// translated to physical sector `partition.start`. Virtual sectors of
    /// sector number `[0, num_sectors)` are accessible.
    ///
    /// `partition.sector_size` must be an integer multiple of
    /// `device.sector_size()`.
    ///
    /// # Panics
    ///
    /// Panics if the partition's sector size is < the device's sector size.
    pub fn new<T>(device: T, partition: Partition) -> CachedPartition
    where
        T: BlockDevice + 'static,
    {
        assert!(partition.sector_size >= device.sector_size());

        CachedPartition {
            device: Box::new(device),
            cache: HashMap::new(),
            partition: partition,
        }
    }

    pub fn flush(&mut self) {
        println!("FLUSHING TO DISK");
        let start_sector = self.partition.start;
        let end_sector = start_sector + self.partition.num_sectors;
        for sector in start_sector..end_sector {
            if self.cache.contains_key(&sector) && self.cache.get(&sector).expect("Couldn't get sector cache").dirty {
                self.cache.get_mut(&sector).expect("Couldn't get sector cache").dirty = false;
                self.write_to_disk(sector).expect("Failed to flush sector to disk");
            }
        }
    }

    /// Returns the number of physical sectors that corresponds to
    /// one logical sector.
    fn factor(&self) -> u64 {
        self.partition.sector_size / self.device.sector_size()
    }

    /// Maps a user's request for a sector `virt` to the physical sector.
    /// Returns `None` if the virtual sector number is out of range.
    fn virtual_to_physical(&self, virt: u64) -> Option<u64> {
        if virt >= self.partition.num_sectors {
            return None;
        }

        let virt_offset = virt - self.partition.start;
        let physical_offset = virt_offset * self.factor();
        let physical_sector = self.partition.start + physical_offset;

        Some(physical_sector)
    }
    
    fn read_to_cache(&mut self, sector: u64) -> io::Result<()> {
        if !self.cache.contains_key(&sector) {
            let phys = match self.virtual_to_physical(sector) {
                Some(phys) => phys,
                None => return ioerr!(NotFound, "Virtual sector doesn't map to physical")
            };
            let factor = self.factor() as usize;
            let phys_size = self.device.sector_size() as usize;
            let mut buf = vec![0u8; factor * phys_size];
            for i in 0..factor {
                self.device.read_sector(phys + i as u64, &mut buf[(phys_size * i)..])?;
            }
            let entry = CacheEntry {
                data: buf,
                dirty: false
            };
            self.cache.insert(sector, entry);
        }
        Ok(())
    }

    fn write_to_disk(&mut self, sector: u64) -> io::Result<()> {
        println!("WRITING TO DISK @ SECTOR {}", sector);
        if self.cache.contains_key(&sector) {
            let phys = match self.virtual_to_physical(sector) {
                Some(phys) => phys,
                None => return ioerr!(NotFound, "Virtual sector doesn't map to physical")
            };
            let factor = self.factor() as usize;
            let phys_size = self.device.sector_size() as usize;
            let buf = &self.cache.get(&sector).expect("Couldn't read cached copy of sector").data;
            for i in 0..factor {
                self.device.write_sector(phys + i as u64, &buf[(phys_size * i)..])?;
            }
        }
        Ok(())
    }

    /// Returns a mutable reference to the cached sector `sector`. If the sector
    /// is not already cached, the sector is first read from the disk.
    ///
    /// The sector is marked dirty as a result of calling this method as it is
    /// presumed that the sector will be written to. If this is not intended,
    /// use `get()` instead.
    ///
    /// # Errors
    ///
    /// Returns an error if there is an error reading the sector from the disk.
    pub fn get_mut(&mut self, sector: u64) -> io::Result<&mut [u8]> {
        self.read_to_cache(sector)?;
        let mut entry = self.cache.get_mut(&sector).unwrap();
        entry.dirty = true;
        println!("Sector {} is dirty", sector);
        Ok(&mut entry.data)
    }

    /// Returns a reference to the cached sector `sector`. If the sector is not
    /// already cached, the sector is first read from the disk.
    ///
    /// # Errors
    ///
    /// Returns an error if there is an error reading the sector from the disk.
    pub fn get(&mut self, sector: u64) -> io::Result<&[u8]> {
        self.read_to_cache(sector)?;
        let entry = self.cache.get(&sector).unwrap();
        Ok(&entry.data)
    }
}

impl Drop for CachedPartition {
    fn drop(&mut self) {
        self.flush();
    }
}

// Implement `BlockDevice` for `CacheDevice`. The `read_sector` and
// `write_sector` methods should only read/write from/to cached sectors.
impl BlockDevice for CachedPartition {
    fn sector_size(&self) -> u64 {
        self.partition.sector_size
    }
    
    fn read_sector(&mut self, sector: u64, buf: &mut [u8]) -> io::Result<usize> {
        let data = self.get(sector)?;
        buf.clone_from_slice(&data);
        Ok(data.len())
    }

    fn write_sector(&mut self, sector: u64, buf: &[u8]) -> io::Result<usize> {
        let data = self.get_mut(sector)?;
        data.clone_from_slice(buf);
        Ok(buf.len())
    }
}

impl fmt::Debug for CachedPartition {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("CachedPartition")
            .field("device", &"<block device>")
            .field("cache", &self.cache)
            .finish()
    }
}
