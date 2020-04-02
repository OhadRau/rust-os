use alloc::string::String;
use alloc::vec::Vec;

use shim::const_assert_size;
use shim::ffi::OsStr;
use shim::io;
use shim::ioerr;

use crate::traits;
use crate::util::VecExt;
use crate::vfat::{Attributes, VFat, Date, Metadata, Time, Timestamp};
use crate::vfat::{Cluster, Entry, File, VFatHandle};

#[derive(Debug)]
pub struct Dir<HANDLE: VFatHandle> {
    pub vfat: HANDLE,
    pub start: Cluster,
    pub meta: Metadata,
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct VFatRegularDirEntry {
    name: [u8; 8],
    ext:  [u8; 3],
    attrs: Attributes,
    __r0: u8,
    created_millis: u8,
    created: Timestamp,
    last_accessed: Date,
    cluster_high: u16,
    modified: Timestamp,
    cluster_low: u16,
    size: u32,
}

const_assert_size!(VFatRegularDirEntry, 32);

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct VFatLfnDirEntry {
    sequence_number: u8,
    name1: [u16; 5],
    attrs: Attributes,
    kind: u8,
    checksum: u8,
    name2: [u16; 6],
    __r0: u16,
    name3: [u16; 2],
}

const_assert_size!(VFatLfnDirEntry, 32);

/* How to determine?
   - Attrs: LFN = READ_ONLY | HIDDEN | SYSTEM | VOLUME_ID
                = 0x01 | 0x02 | 0x04 | 0x08 = 0x0F
*/
#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct VFatUnknownDirEntry {
    valid: u8,
    __r0: [u8; 10],
    attrs: Attributes,
    __r1: [u8; 20],
}

const_assert_size!(VFatUnknownDirEntry, 32);

pub union VFatDirEntry {
    unknown: VFatUnknownDirEntry,
    regular: VFatRegularDirEntry,
    long_filename: VFatLfnDirEntry,
}

impl<HANDLE: VFatHandle> Dir<HANDLE> {
    /// Finds the entry named `name` in `self` and returns it. Comparison is
    /// case-insensitive.
    ///
    /// # Errors
    ///
    /// If no entry with name `name` exists in `self`, an error of `NotFound` is
    /// returned.
    ///
    /// If `name` contains invalid UTF-8 characters, an error of `InvalidInput`
    /// is returned.
    pub fn find<P: AsRef<OsStr>>(&self, name: P) -> io::Result<Entry<HANDLE>> {
        use traits::Dir;

        let name = match name.as_ref().to_str() {
            Some(name) => name,
            None => return ioerr!(InvalidInput, "Invalid name provided to find")
        };
        for entry in self.entries()? {
            let entry_name = match &entry {
                Entry::Dir(dir) => &dir.meta.name,
                Entry::File(file) => &file.meta.name,
            };
            if entry_name.eq_ignore_ascii_case(name) {
                return Ok(entry)
            }
        }
        ioerr!(NotFound, "Could not find entry with provided name")
    }
}

pub struct DirIter<HANDLE: VFatHandle> {
    vfat: HANDLE,
    entries: Vec<VFatDirEntry>,
    index: usize,
}

impl<HANDLE: VFatHandle> DirIter<HANDLE> {
    fn get_meta(&mut self) -> Option<(Metadata, Cluster)> {
        // Max 256 characters in filename but we add 13 chars at a time...
        // 260 is the smallest multiple of 13 that can fit 255 chars
        let mut name_sequence = [0u16; 260];
        let mut long_name = false;

        loop {
            if self.index >= self.entries.len() {
                return None
            }
            let entry = &self.entries[self.index];
            self.index += 1;

            let unknown = unsafe { entry.unknown };

            if unknown.valid == 0x00 {
                return None
            } else if unknown.valid == 0xE5 {
                long_name = false;
                name_sequence = [0u16; 260];
                continue
            }

            let is_lfn = unknown.attrs.is_lfn();

            if is_lfn {
                let lfn = unsafe { entry.long_filename };
                
                let seq_num = ((lfn.sequence_number & 0x1F) - 1) as usize;
                name_sequence[seq_num * 13      .. seq_num * 13 + 5 ].copy_from_slice(&{lfn.name1});
                name_sequence[seq_num * 13 + 5  .. seq_num * 13 + 11].copy_from_slice(&{lfn.name2});
                name_sequence[seq_num * 13 + 11 .. seq_num * 13 + 13].copy_from_slice(&{lfn.name3});
                
                long_name = true;
            } else {
                let reg = unsafe { entry.regular };
                
                let name = if long_name {
                    let mut end_index = 0;
                    for byte in name_sequence.iter() {
                        if *byte == 0 {
                            break
                        }
                        end_index += 1;
                    }
                    String::from_utf16(&name_sequence[0..end_index]).unwrap()
                } else {
                    let base = core::str::from_utf8(&reg.name).unwrap().trim_end();
                    let ext = core::str::from_utf8(&reg.ext).unwrap().trim_end();
                    if ext.is_empty() {
                        String::from(base)
                    } else {
                        format!("{}.{}", base, ext)
                    }
                };

                let created = reg.created;
                let accessed = Timestamp {
                    time: Time::default(),
                    date: reg.last_accessed,
                };
                let modified = reg.modified;

                let attributes = reg.attrs;

                let lo = reg.cluster_low as u32;
                let hi = reg.cluster_high as u32;
                let cluster = Cluster::from(lo | (hi << 16));
                let size = reg.size as usize;

                let meta = Metadata {
                    name,
                    created,
                    accessed,
                    modified,
                    attributes,
                    size,
                };

                return Some((meta, cluster))
            }
        }
    }
}

impl<HANDLE: VFatHandle> Iterator for DirIter<HANDLE> {
    type Item = Entry<HANDLE>;

    fn next(&mut self) -> Option<Self::Item> {
        use crate::vfat::Pos;

        let (meta, cluster) = self.get_meta()?;
        if meta.attributes.is_dir() {
            let dir = Dir {
                vfat: self.vfat.clone(),
                start: cluster,
                meta,
            };
            Some(Entry::Dir(dir))
        } else {
            let file = File {
                vfat: self.vfat.clone(),
                start: cluster,
                meta,
                pos: Pos {
                    cluster,
                    offset: 0,
                },
                amt_read: 0,
            };
            Some(Entry::File(file))
        }
    }
}

impl<HANDLE: VFatHandle> traits::Dir for Dir<HANDLE> {
    /// The type of entry stored in this directory.
    type Entry = Entry<HANDLE>;

    /// An type that is an iterator over the entries in this directory.
    type Iter = DirIter<HANDLE>;

    /// Returns an interator over the entries in this directory.
    fn entries(&self) -> io::Result<Self::Iter> {
        let mut buf = Vec::new();
        self.vfat.lock(|vfat: &mut VFat<HANDLE>| -> io::Result<()> {
            vfat.read_chain(self.start, &mut buf)?;
            Ok(())
        })?;
        
        let entries = unsafe { buf.cast::<VFatDirEntry>() };
        
        Ok(DirIter {
            vfat: self.vfat.clone(),
            entries,
            index: 0,
        })
    }
}
