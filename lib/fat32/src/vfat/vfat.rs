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
}

#[derive(Debug, Copy, Clone)]
pub struct Pos {
    pub cluster: Cluster,
    pub offset: usize,
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

        let num_sectors = ebpb.num_fats as u64 * ebpb.sectors_per_fat as u64;

        let sector_size = cached.sector_size() as u16;

        let vfat = VFat {
            phantom: PhantomData,
            device: cached,
            bytes_per_sector: sector_size,
            sectors_per_cluster: ebpb.sectors_per_cluster,
            sectors_per_fat: ebpb.sectors_per_fat,
            fat_start_sector: start_sector as u64 + ebpb.num_reserved_sectors as u64,
            data_start_sector: start_sector as u64 + ebpb.num_reserved_sectors as u64 + num_sectors,
            rootdir_cluster: Cluster::from(ebpb.root_cluster_number)
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

    pub fn seek(&mut self, mut base: Pos, mut offset: usize) -> io::Result<Pos> {
        let cluster_size =
            (self.bytes_per_sector as usize) * (self.sectors_per_cluster as usize);
        
        while offset >= cluster_size {
            offset -= cluster_size;
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
        base.offset = offset;
        Ok(base)
    }

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
    //  * A method to return a reference to a `FatEntry` for a cluster where the
    //    reference points directly into a cached sector.
    //
    pub fn fat_entry(&mut self, cluster: Cluster) -> io::Result<&FatEntry> {
        let (sector, offset) = self.lookup_entry(cluster);
        let fat = self.device.get(sector)?;
        let entries = unsafe { &fat.cast::<FatEntry>() };
        Ok(&entries[offset])
    }

    pub fn bytes_per_cluster(&self) -> usize {
        self.bytes_per_sector as usize * self.sectors_per_cluster as usize
    }
}

impl<'a, HANDLE: VFatHandle> FileSystem for &'a HANDLE {
    type File = File<HANDLE>;
    type Dir = Dir<HANDLE>;
    type Entry = Entry<HANDLE>;

    fn open<P: AsRef<Path>>(self, path: P) -> io::Result<Self::Entry> {
        use crate::traits::Entry;
        use crate::vfat::Metadata;

        let mut entry = self.lock(|vfat: &mut VFat<HANDLE>| -> Self::Entry { 
            crate::vfat::Entry::Dir(Dir {
                vfat: self.clone(),
                start: vfat.rootdir_cluster,
                meta: Metadata::default(),
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
}
