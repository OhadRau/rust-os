use core::fmt::Debug;
use core::marker::PhantomData;
use core::mem::size_of;

use alloc::vec::Vec;

use shim::{io, ioerr};
use shim::path;
use shim::path::Path;

use crate::mbr::MasterBootRecord;
use crate::traits::{BlockDevice, FileSystem};
use crate::util::SliceExt;
use crate::vfat::{BiosParameterBlock, CachedPartition, Partition};
use crate::vfat::{Cluster, Dir, Entry, Error, FatEntry, File, Status};

/// A generic trait that handles a critical section as a closure
pub trait VFatHandle: Clone + Debug + Send + Sync {
    fn new(val: VFat<Self>) -> Self;
    fn lock<R>(&self, f: impl FnOnce(&mut VFat<Self>) -> R) -> R;
}

#[derive(Debug)]
pub struct VFat<HANDLE: VFatHandle> {
    phantom: PhantomData<HANDLE>,
    device: CachedPartition,
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    sectors_per_fat: u32,
    fat_start_sector: u64,
    data_start_sector: u64,
    rootdir_cluster: Cluster,
    num_fats: u8,
}

#[derive(Debug, Copy, Clone)]
pub struct Pos {
    pub cluster: Cluster,
    pub offset: usize,
}

#[derive(Debug, Copy, Clone)]
pub struct Range {
    pub start: Pos,
    pub end: Pos,
}

impl<HANDLE: VFatHandle> VFat<HANDLE> {
    pub fn from<T>(mut device: T) -> Result<HANDLE, Error>
    where
        T: BlockDevice + 'static,
    {
        let mbr = MasterBootRecord::from(&mut device)?;
        let start_sector = match mbr.find_vfat_partition() {
            Some(start) => start,
            None => return Err(Error::NotFound)
        };
        let ebpb = BiosParameterBlock::from(&mut device, start_sector as u64)?;
        let partition = Partition {
            start: start_sector as u64,
            num_sectors: ebpb.num_logical_sectors_ext as u64,
            sector_size: ebpb.bytes_per_sector as u64,
        };
        let cached = CachedPartition::new(device, partition);

        let num_fats = ebpb.num_fats;
        let num_sectors = num_fats as u64 * ebpb.sectors_per_fat as u64;

        let sector_size = cached.sector_size() as u16;

        let vfat = VFat {
            phantom: PhantomData,
            device: cached,
            bytes_per_sector: sector_size,
            sectors_per_cluster: ebpb.sectors_per_cluster,
            sectors_per_fat: ebpb.sectors_per_fat,
            fat_start_sector: start_sector as u64 + ebpb.num_reserved_sectors as u64,
            data_start_sector: start_sector as u64 + ebpb.num_reserved_sectors as u64 + num_sectors,
            rootdir_cluster: Cluster::from(ebpb.root_cluster_number),
            num_fats,
        };
        Ok(HANDLE::new(vfat))
    }

    fn lookup_entry(&self, cluster: Cluster) -> (u64, usize) {
        let num = cluster.num();
        let entry_size = size_of::<FatEntry>() as u64;
        let entries_per_sector = self.bytes_per_sector as u64 / entry_size;
        let sector = num as u64 / entries_per_sector;
        let offset = num as usize % entries_per_sector as usize;
        (self.fat_start_sector as u64 + sector, offset)
    }

    fn cluster_start_sector(&self, cluster: Cluster) -> u64 {
        let num = (cluster.num() - 2) as u64;
        let sector_offset = num * (self.sectors_per_cluster as u64);
        self.data_start_sector as u64 + sector_offset
    }

    //
    //  * A method to read from an offset of a cluster into a buffer.
    //
    pub fn read_cluster(&mut self, cluster: Cluster, offset: usize,
                        buf: &mut [u8]) -> io::Result<usize> {
        use core::cmp::min;

        match self.fat_entry(cluster)?.status() {
            Status::Data(_) | Status::Eoc(_) => (),
            _ => return ioerr!(Other, "Tried to read from invalid cluster")
        }

        let start_sector = self.cluster_start_sector(cluster);
        let bytes_per_sector = self.bytes_per_sector as usize;
        let sector_num = offset / bytes_per_sector;
        let sector_off = offset % bytes_per_sector;
        let bytes_per_cluster = bytes_per_sector * self.sectors_per_cluster as usize;
        let read_size = min(buf.len(), bytes_per_cluster - offset);

        let mut bytes_read = 0;
        let mut sector = start_sector + sector_num as u64;
        while bytes_read < read_size {
            let bytes = self.device.get(sector)?;

            if bytes_read == 0 {
                let to_read = min(buf.len(), bytes[sector_off..].len());
                buf[..to_read].copy_from_slice(&bytes[sector_off..sector_off + to_read]);
                bytes_read += to_read;
            } else {
                let to_read = min(buf.len() - bytes_read, bytes.len());
                buf[bytes_read..bytes_read+to_read].copy_from_slice(&bytes[..to_read]);
                bytes_read += to_read;
            }

            sector += 1;
        }

        Ok(read_size)
    }
    
    //
    //  * A method to write from an offset of a cluster into a buffer.
    //
    pub fn write_cluster(&mut self, cluster: Cluster, offset: usize,
                         buf: &[u8]) -> io::Result<usize> {
        use core::cmp::min;

        match self.fat_entry(cluster)?.status() {
            Status::Data(_) | Status::Eoc(_) => (),
            _ => return ioerr!(Other, "Tried to read from invalid cluster")
        }

        let start_sector = self.cluster_start_sector(cluster);

        #[cfg(debug_assertions)]
        println!("Cluster {:?}/offset {} => start_sector {}", cluster, offset, start_sector);

        let bytes_per_sector = self.bytes_per_sector as usize;
        let sector_num = offset / bytes_per_sector;
        let sector_off = offset % bytes_per_sector;
        let bytes_per_cluster = bytes_per_sector * self.sectors_per_cluster as usize;
        let write_size = min(buf.len(), bytes_per_cluster - offset);

        #[cfg(debug_assertions)]
        println!("Write size: {}", write_size);

        let mut bytes_written = 0;
        let mut sector = start_sector + sector_num as u64;
        while bytes_written < write_size {
            let bytes = self.device.get_mut(sector)?;

            if bytes_written == 0 {
                let to_write = min(buf.len(), bytes[sector_off..].len());

                #[cfg(debug_assertions)]
                println!("to_write: {} | range: {}..{}", to_write, sector_off, sector_off + to_write);

                bytes[sector_off..sector_off + to_write].copy_from_slice(&buf[..to_write]);
                bytes_written += to_write;
            } else {
                let to_write = min(buf.len() - bytes_written, bytes.len());
                bytes[..to_write].copy_from_slice(&buf[bytes_written..bytes_written+to_write]);
                bytes_written += to_write;
            }

            sector += 1;
        }

        Ok(write_size)
    }

    pub fn seek(&mut self, mut base: Pos, mut offset: usize) -> io::Result<Pos> {
        let cluster_size =
            (self.bytes_per_sector as usize) * (self.sectors_per_cluster as usize);
        
        while offset >= cluster_size {
            // AVOID UNDERFLOW! (512 - 1024) as usize > 0usize
            offset = offset.saturating_sub(cluster_size);
            match self.fat_entry(base.cluster)?.status() {
                Status::Eoc(_) => {
                    if offset > 0 {
                        return ioerr!(InvalidInput, "Tried to seek past end of file")
                    }
                },
                Status::Data(next) => {
                    base.cluster = next;
                    base.offset = 0;
                },
                _ => return ioerr!(InvalidData, "Couldn't read cluster in chain")
            }
        }
        base.offset += offset;
        Ok(base)
    }

    pub fn seek_and_extend(&mut self, mut base: Pos, mut offset: usize) -> io::Result<Pos> {
        let cluster_size =
            (self.bytes_per_sector as usize) * (self.sectors_per_cluster as usize);

        while offset >= cluster_size {
            // AVOID UNDERFLOW! (512 - 1024) as usize > 0usize
            offset = offset.saturating_sub(cluster_size);
            match self.fat_entry(base.cluster)?.status() {
                Status::Eoc(_) => {
                    let next_cluster =
                        self.alloc_cluster(Status::Eoc(0)).expect("Couldn't allocate next cluster");
                    self.set_fat_entry(base.cluster, Status::Data(next_cluster)).expect("Couldn't update FAT entry");
                    base.cluster = next_cluster;
                    base.offset = 0;
                },
                Status::Data(next) => {
                    base.cluster = next;
                    base.offset = 0;
                },
                _ => return ioerr!(InvalidData, "Couldn't read cluster in chain")
            }
        }
        base.offset += offset;
        Ok(base)
    }

    //
    //  * A method to read all of the clusters chained from a starting position
    //    into a vector.
    //
    pub fn read_chain_pos(&mut self, mut pos: Pos,
                          buf: &mut [u8]) -> io::Result<usize> {
        let mut bytes_read = 0;

        loop {
            if bytes_read >= buf.len() {
                return Ok(bytes_read)
            }

            let num_read = self.read_cluster(pos.cluster, pos.offset, &mut buf[bytes_read..])?;

            bytes_read += num_read;
            match self.fat_entry(pos.cluster)?.status() {
                Status::Eoc(_) => {
                    return Ok(bytes_read)
                },
                Status::Data(next) => {
                    pos.offset = 0;
                    pos.cluster = next;
                },
                _ => return ioerr!(InvalidData, "Couldn't read cluster in chain")
            }
        }
    }

    //
    //  * A method to read all of the clusters chained from a starting cluster
    //    into a vector.
    //

    pub fn read_chain(&mut self, start: Cluster, buf: &mut Vec<u8>) -> io::Result<usize> {
        let cluster_size =
            (self.bytes_per_sector as usize) * (self.sectors_per_cluster as usize);
        let mut cluster = start;
        let mut bytes_read = 0;
        loop {
            let mut cluster_buf = vec![0u8; cluster_size];
            let num_read = self.read_cluster(cluster, 0, &mut cluster_buf)?;
            buf.extend_from_slice(&cluster_buf);

            bytes_read += num_read;
            match self.fat_entry(start)?.status() {
                Status::Eoc(_) => return Ok(bytes_read),
                Status::Data(next) => {
                    if next == cluster {
                        return Ok(bytes_read);
                    }
                    cluster = next;
                },
                _ => return ioerr!(InvalidData, "Couldn't read cluster in chain")
            }
        }
    }

    //
    //  * A method to write all into the clusters chained from a starting position
    //    from a vector.
    //
    pub fn write_chain_pos(&mut self, mut pos: Pos,
                           buf: &[u8]) -> io::Result<usize> {
        let mut bytes_written = 0;

        #[cfg(debug_assertions)]
        println!("WRITING FROM {:?}", pos);
        #[cfg(debug_assertions)]
        println!("Buffer length: {}", buf.len());

        loop {
            if bytes_written >= buf.len() {
                return Ok(bytes_written)
            }

            let num_written = self.write_cluster(pos.cluster, pos.offset, &buf[bytes_written..])?;

            #[cfg(debug_assertions)]
            println!("Just wrote {} bytes to {:?}", num_written, pos);

            bytes_written += num_written;
            match self.fat_entry(pos.cluster)?.status() {
                Status::Eoc(_) => {
                    if bytes_written < buf.len() {
                        let next_cluster =
                            self.alloc_cluster(Status::Eoc(0)).expect("Couldn't allocate next cluster");
                        self.set_fat_entry(pos.cluster, Status::Data(next_cluster)).expect("Couldn't update FAT entry");
                        pos.offset = 0;
                        pos.cluster = next_cluster;
                    }
                },
                Status::Data(next) => {
                    pos.offset = 0;
                    pos.cluster = next;
                },
                _ => return ioerr!(InvalidData, "Couldn't write cluster in chain")
            }
        }
    }

    //
    //  * A method to return a reference to a `FatEntry` for a cluster where the
    //    reference points directly into a cached sector.
    //
    pub fn fat_entry(&mut self, cluster: Cluster) -> io::Result<&FatEntry> {
        let (sector, offset) = self.lookup_entry(cluster);
        let fat = self.device.get(sector)?;
        let entries = unsafe { &fat.cast::<FatEntry>() };
        Ok(&entries[offset])
    }

    // Replace a FatEntry on the disk
    pub fn set_fat_entry(&mut self, cluster: Cluster, new_status: Status) -> Option<()> {
        let (sector, offset) = self.lookup_entry(cluster);
        let fat = self.device.get_mut(sector).ok()?;
        let entries = unsafe { &mut fat.cast_mut::<FatEntry>() };
        entries[offset] = FatEntry::from_status(new_status);
        Some(())
    }

    // Find the first unused FatEntry on the disk
    pub fn find_free_entry(&mut self) -> Option<Cluster> {
        let num_clusters =
            self.sectors_per_fat * (self.bytes_per_sector as u32) / (core::mem::size_of::<FatEntry>() as u32);
        for i in 0..num_clusters {
            let cluster = Cluster::from(i);
            match self.fat_entry(cluster).expect("Couldn't read FAT entry").status() {
                Status::Free => return Some(cluster),
                _ => continue
            }
        }
        None
    }

    // Allocate a cluster, updating its FatEntry to the requested status
    pub fn alloc_cluster(&mut self, new_status: Status) -> Option<Cluster> {
        let cluster = self.find_free_entry()?;
        self.set_fat_entry(cluster, new_status)?;
        Some(cluster)
    }

    // Free a cluster, updating its FatEntry to show that it's free
    pub fn free_cluster(&mut self, cluster: Cluster) -> Option<()> {
        self.set_fat_entry(cluster, Status::Free)?;
        Some(())
    }

    pub fn bytes_per_cluster(&self) -> usize {
        self.bytes_per_sector as usize * self.sectors_per_cluster as usize
    }

    // wrapper to give users of the filesystem ability to flush it
    pub fn flush(&mut self) {
        self.device.flush();
    }
}

impl<'a, HANDLE: VFatHandle> FileSystem for &'a HANDLE {
    type File = File<HANDLE>;
    type Dir = Dir<HANDLE>;
    type Entry = Entry<HANDLE>;
    type Metadata = crate::vfat::Metadata;

    fn open<P: AsRef<Path>>(self, path: P) -> io::Result<Self::Entry> {
        use crate::traits::Entry;
        use crate::vfat::Metadata;

        let mut entry = self.lock(|vfat: &mut VFat<HANDLE>| -> Self::Entry { 
            crate::vfat::Entry::Dir(Dir {
                vfat: self.clone(),
                start: vfat.rootdir_cluster,
                meta: Metadata::default(),
                entry: None,
            })
        });
        for comp in path.as_ref().components() {
            match comp {
                path::Component::RootDir => (),
                path::Component::Normal(item) => {
                    match entry.as_dir() {
                        Some(dir) =>
                            entry = dir.find(item)?,
                        None =>
                            return ioerr!(InvalidInput, "Attempted to use file as a directory"),
                    }
                },
                _ => return ioerr!(InvalidInput, "Unknown path component")
            }
        }
        
        Ok(entry)
    }

    fn flush(self) {
        self.lock(|vfat: &mut VFat<HANDLE>| {
            vfat.flush();
        });
    }
}
