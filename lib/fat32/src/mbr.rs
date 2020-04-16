use core::fmt;
use shim::const_assert_size;
use shim::io;
use core::mem;

use crate::traits::BlockDevice;

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct CHS {
    pub head: u8,
    pub sector_cylinder: u16,
}

// implement Debug for CHS
impl fmt::Debug for CHS {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sector = self.sector_cylinder >> 6;
        let cylinder = self.sector_cylinder & 0b1111111111;
        fmt.debug_struct("CHS")
            .field("head", &{self.head})
            .field("sector", &sector)
            .field("cylinder", &cylinder)
            .finish()
    }
}

const_assert_size!(CHS, 3);

#[repr(C, packed)]
pub struct PartitionEntry {
    pub boot_indicator: u8,
    pub start: CHS,
    pub partition_type: u8,
    pub end: CHS,
    pub relative_sector: u32,
    pub total_sectors: u32,
}

// implement Debug for PartitionEntry
impl fmt::Debug for PartitionEntry {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("PartitionEntry")
            .field("boot_indicator", &{self.boot_indicator})
            .field("start", &{self.start})
            .field("partition_type", &{self.partition_type})
            .field("end", &{self.end})
            .field("relative_sector", &{self.relative_sector})
            .field("total_sectors", &{self.total_sectors})
            .finish()
    }
}

const_assert_size!(PartitionEntry, 16);

/// The master boot record (MBR).
#[repr(C, packed)]
pub struct MasterBootRecord {
    pub bootstrap: [u8; 436],
    pub disk_id: [u8; 10],
    pub partition_table: [PartitionEntry; 4],
    pub signature: [u8; 2],
}

impl Default for MasterBootRecord {
    fn default() -> MasterBootRecord {
        unsafe { mem::transmute::<[u8; 512], MasterBootRecord>([0u8; 512])}
    }
}

// implement Debug for MasterBootRecord
impl fmt::Debug for MasterBootRecord {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("MasterBootRecord")
            //.field("bootstrap", &self.bootstrap)
            .field("disk_id", &{self.disk_id})
            .field("partition_table", &self.partition_table)
            .field("signature", &{self.signature})
            .finish()
    }
}

const_assert_size!(MasterBootRecord, 512);

#[derive(Debug)]
pub enum Error {
    /// There was an I/O error while reading the MBR.
    Io(io::Error),
    /// Partiion `.0` (0-indexed) contains an invalid or unknown boot indicator.
    UnknownBootIndicator(u8),
    /// The MBR magic signature was invalid.
    BadSignature,
}

impl MasterBootRecord {
    /// Reads and returns the master boot record (MBR) from `device`.
    ///
    /// # Errors
    ///
    /// Returns `BadSignature` if the MBR contains an invalid magic signature.
    /// Returns `UnknownBootIndicator(n)` if partition `n` contains an invalid
    /// boot indicator. Returns `Io(err)` if the I/O error `err` occured while
    /// reading the MBR.
    pub fn from<T: BlockDevice>(mut device: T) -> Result<MasterBootRecord, Error> {
        let mut buf = [0u8; 512];
        match device.read_sector(0, &mut buf) {
            Err(error) => return Err(Error::Io(error)),
            Ok(_) => ()
        }

        for partition in 0..4 {
            match buf[446 + partition * 16] {
                0x0 => (),
                0x80 => (),
                _ => return Err(Error::UnknownBootIndicator(partition as u8))
            }
        }

        match &buf[510..=511] {
            [0x55, 0xAA] => (),
            _ => return Err(Error::BadSignature)
        }

        let mbr = unsafe {
            core::mem::transmute::<[u8; 512], MasterBootRecord>(buf)
        };
        Ok(mbr)
    }

    pub fn find_vfat_partition(&self) -> Option<u32> {
        for part in self.partition_table.iter() {
            if part.partition_type == 0xB || part.partition_type == 0xC {
                return Some(part.relative_sector)
            }
        }
        None
    }

    // gonna do 1-indexed for now
    // returns None if the selected partition is not vfat or is out of range
    pub fn get_partition_start(&self, part_num: usize) -> Option<u32> {
        if part_num < 1 || part_num > 4 {
            return None;
        }
        
        let part = &self.partition_table[part_num - 1];

        if part.partition_type == 0xB || part.partition_type == 0xC {
            Some(part.relative_sector)
        } else {
            None
        }
    }

    pub fn get_partition_size(&self, part_num: usize) -> Option<u64> {
        if part_num < 1 || part_num > 4 {
            return None;
        }

        Some(self.partition_table[part_num - 1].total_sectors as u64 * 512)
    }
}
