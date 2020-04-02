use core::fmt;
use shim::const_assert_size;

use crate::traits::BlockDevice;
use crate::vfat::Error;

#[repr(C, packed)]
pub struct BiosParameterBlock {
        __r0: [u8; 3],
    pub oem_id: [u8; 8],
    pub bytes_per_sector: u16,
    pub sectors_per_cluster: u8,
    pub num_reserved_sectors: u16,
    pub num_fats: u8,
    pub max_dir_entries: u16,
    pub num_logical_sectors: u16,
    pub fat_id: u8,
        __r1: u16,
    pub sectors_per_track: u16,
    pub num_heads: u16,
    pub num_hidden_sectors: u32,
    pub num_logical_sectors_ext: u32,
    pub sectors_per_fat: u32,
    pub flags: u16,
    pub fat_version: u16,
    pub root_cluster_number: u32,
    pub fs_info_sector_number: u16,
    pub backup_boot_sector_number: u16,
        __r2: [u8; 12],
    pub drive_number: u8,
        __r3: u8,
    pub signature: u8,
    pub volume_id: u32,
    pub volume_label: [u8; 11],
    pub system_id: [u8; 8],
    pub boot_code: [u8; 420],
    pub boot_signature: u16,
}

const_assert_size!(BiosParameterBlock, 512);

impl BiosParameterBlock {
    /// Reads the FAT32 extended BIOS parameter block from sector `sector` of
    /// device `device`.
    ///
    /// # Errors
    ///
    /// If the EBPB signature is invalid, returns an error of `BadSignature`.
    pub fn from<T: BlockDevice>(mut device: T, sector: u64) -> Result<BiosParameterBlock, Error> {
        let mut buf = [0u8; 512];
        device.read_sector(sector, &mut buf)?;
        let ebpb: BiosParameterBlock = unsafe { core::mem::transmute(buf) };
        if ebpb.signature != 0x28 && ebpb.signature != 0x29 {
            return Err(Error::BadSignature);
        }
        if ebpb.boot_signature != 0xAA55 {
            return Err(Error::BadSignature);
        }
        Ok(ebpb)
    }
}

impl fmt::Debug for BiosParameterBlock {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("BiosParameterBlock")
            .field("oem_id", &{self.oem_id})
            .field("bytes_per_sector", &{self.bytes_per_sector})
            .field("sectors_per_cluster", &{self.sectors_per_cluster})
            .field("num_reserved_sectors", &{self.num_reserved_sectors})
            .field("num_fats", &{self.num_fats})
            .field("max_dir_entries", &{self.max_dir_entries})
            .field("num_logical_sectors", &{self.num_logical_sectors})
            .field("fat_id", &{self.fat_id})
            .field("sectors_per_track", &{self.sectors_per_track})
            .field("num_heads", &{self.num_heads})
            .field("num_hidden_sectors", &{self.num_hidden_sectors})
            .field("num_logical_sectors_ext", &{self.num_logical_sectors_ext})
            .field("sectors_per_fat", &{self.sectors_per_fat})
            .field("flags", &{self.flags})
            .field("fat_version", &{self.fat_version})
            .field("root_cluster_number", &{self.root_cluster_number})
            .field("fs_info_sector_number", &{self.fs_info_sector_number})
            .field("backup_boot_sector_number", &{self.backup_boot_sector_number})
            .field("drive_number", &{self.drive_number})
            .field("signature",  &format_args!("{:#x}", &{self.signature}))
            .field("volume_id", &{self.volume_id})
            .field("volume_label", unsafe { &core::str::from_utf8_unchecked(&{self.volume_label}) })
            .field("system_id", unsafe { &core::str::from_utf8_unchecked(&{self.system_id}) })
            .field("boot_signature", &format_args!("{:#x}", &{self.boot_signature}))
            .finish()
    }
}
