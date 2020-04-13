use alloc::string::String;
use alloc::vec::Vec;

use shim::const_assert_size;
use shim::ffi::OsStr;
use shim::io;
use shim::ioerr;

use crate::traits;
use crate::util::VecExt;
use crate::vfat::{Attributes, VFat, Date, Metadata, Time, Timestamp};
use crate::vfat::{Cluster, Entry, File, VFatHandle, Pos};

#[derive(Debug)]
pub struct Dir<HANDLE: VFatHandle> {
    pub vfat: HANDLE,
    pub start: Cluster,
    pub meta: Metadata,
    pub entry: Option<Pos>,
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

impl<HANDLE: VFatHandle> From<&File<HANDLE>> for VFatRegularDirEntry {
    fn from(file: &File<HANDLE>) -> VFatRegularDirEntry {
        let (name, ext) = get_short_name(file.meta.name.clone());
        VFatRegularDirEntry {
            name,
            ext,
            attrs: file.meta.attributes,
            __r0: 0,
            created_millis: 0, // force this field to 0 for now
            created: file.meta.created,
            last_accessed: file.meta.accessed.date,
            cluster_high: (file.start.num() >> 16) as u16,
            modified: file.meta.modified,
            cluster_low: (file.start.num() & 0xFFFF) as u16,
            size: file.meta.size as u32
        }
    }
}


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

fn get_short_name(name: String) -> ([u8; 8], [u8; 3]) {
    let parts = name.split('.').collect::<Vec<_>>();
    let mut name = [0u8; 8];
    let name_chars = format!("{: <8}", parts[0]);
    name.copy_from_slice(&name_chars.as_bytes()[0..8]);
    let mut ext = [0u8; 3];
    let ext_chars = format!("{: <3}", if parts.len() > 1 { parts[1] } else { "" });
    ext.copy_from_slice(&ext_chars.as_bytes()[0..3]);
    (name, ext)
}

fn get_checksum(name: String) -> u8 {
    let mut sum = 0u8;
    let (name, ext) = get_short_name(name);
    #[cfg(debug_assertions)]
    print!("Calculating checksum: ");

    for ch in name.iter().chain(&ext) {
        sum = ((sum & 1) << 7).wrapping_add(sum >> 1).wrapping_add(*ch);
    }

    #[cfg(debug_assertions)]
    println!("");

    sum
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

    fn get_start_pos(&mut self, prev_index: usize) -> io::Result<Pos> {
        /* Oh no, what if we're at the end of an EOC? Where do we write the new entry? write_chain_pos
           won't work because it's starting in an undefined region? Good question! A hack we can do is
           start from the beginning of the file, seek to the new position, and then have the seek
           extend the dir as necessary */
        let entry_size = core::mem::size_of::<VFatUnknownDirEntry>();

        let file_base_pos = Pos {
            cluster: self.start,
            offset: 0,
        };

        #[cfg(debug_assertions)]
        println!("prev_index: {}, entry_size: {}, seeking to: {}", prev_index, entry_size, prev_index * entry_size);
        //assert_eq!(prev_index * entry_size, self.meta.size);

        self.vfat.lock(|vfat: &mut VFat<HANDLE>| -> io::Result<Pos> {
            vfat.seek_and_extend(file_base_pos, prev_index * entry_size)
        })
    }

    fn blank_dir(dir: Cluster, parent: Option<Cluster>) -> Vec<u8> {
        let mut buffer = vec![0u8; 1024];

        let empty_ext = [' ' as u8; 3];
        let mut dot_name = [' ' as u8; 8];
        let mut dotdot_name = [' ' as u8; 8];
        dot_name[0] = '.' as u8;
        dotdot_name[0] = '.' as u8;
        dotdot_name[1] = '.' as u8;

        let cluster_high = ((dir.num() & 0xFFFF0000) >> 16) as u16;
        let cluster_low  = (dir.num() & 0xFFFF) as u16;
        let dot = VFatRegularDirEntry {
            name: dot_name,
            ext: empty_ext,
            attrs: Attributes::default().dir(), // do we need to hide it?
            __r0: 0,
            created_millis: 0,
            created: Timestamp::default(),
            last_accessed: Date::default(),
            cluster_high,
            modified: Timestamp::default(),
            cluster_low,
            size: 0,
        };
        
        let cluster_high = if let Some(ploc) = parent {
            ((ploc.num() & 0xFFFF0000) >> 16) as u16
        } else { 0 };
        let cluster_low  = if let Some(ploc) = parent {
            (ploc.num() & 0xFFFF) as u16
        } else { 0 };
        let dotdot = VFatRegularDirEntry {
            name: dotdot_name,
            ext: empty_ext,
            attrs: Attributes::default().dir(), // do we need to hide it?
            __r0: 0,
            created_millis: 0,
            created: Timestamp::default(),
            last_accessed: Date::default(),
            cluster_high,
            modified: Timestamp::default(),
            cluster_low,
            size: 0,
        };
        let entries = vec![dot, dotdot];
        let entries_buf = unsafe { entries.cast::<u8>() };

        buffer[0..entries_buf.len()].copy_from_slice(&entries_buf);

        buffer
    }

    fn create_entry(&mut self, meta: Metadata, start: Pos, parent: Option<Cluster>) -> io::Result<Entry<HANDLE>> {
        use crate::vfat::Status;
        use io::{Error, ErrorKind};

        let (name, ext) = get_short_name(meta.name.clone());

        // We always create files as empty, so handle the empty cases for files & dirs
        let location = if meta.attributes.is_dir() {
            self.vfat.lock(|vfat: &mut VFat<HANDLE>| -> io::Result<Cluster> {
                let cluster =
                    vfat.alloc_cluster(Status::Eoc(0)).ok_or(Error::new(ErrorKind::AddrInUse, "Couldn't find free cluster"))?;
                vfat.write_cluster(cluster, 0, &Self::blank_dir(cluster, parent))?;
                Ok(cluster)
            })?
        } else {
            Cluster::from(0)
        };
        //println!("Allocated {:?} for new file", location);
        let cluster_high = ((location.num() & 0xFFFF0000) >> 16) as u16;
        let cluster_low  = (location.num() & 0xFFFF) as u16;
        let entry = VFatRegularDirEntry {
            name,
            ext,
            attrs: meta.attributes,
            __r0: 0,
            created_millis: 0,
            created: Timestamp::default(),
            last_accessed: Date::default(),
            cluster_high,
            modified: Timestamp::default(),
            cluster_low,
            size: 0,
        };

        let new_entry = vec![entry];
        let entry_size = core::mem::size_of::<VFatRegularDirEntry>();
        let buf = unsafe { new_entry.cast::<u8>() };
        self.vfat.lock(|vfat: &mut VFat<HANDLE>| -> io::Result<()> {
            vfat.write_chain_pos(start, &buf[0..entry_size])?;
            Ok(())
        })?;

        if meta.attributes.is_dir() {
            let dir = Dir {
                vfat: self.vfat.clone(),
                start: location,
                meta,
                entry: Some(start),
            };
            Ok(Entry::Dir(dir))
        } else {
            let file = File {
                vfat: self.vfat.clone(),
                start: location,
                meta,
                entry: Some(start),
                pos: Pos {
                    cluster: location,
                    offset: 0,
                },
                amt_read: 0,
            };
            Ok(Entry::File(file))
        }
    }

    fn create_lfn_entry(&mut self, meta: Metadata, mut start: Pos, parent: Option<Cluster>) -> io::Result<Entry<HANDLE>> {
        use core::cmp::min;
        use crate::util::SliceExt;

        // Calculate the checksum for the name
        let checksum = get_checksum(meta.name.clone());
        // Encode the name characters as UTF-16 (UCS-2)
        let name_sequence = meta.name.encode_utf16().collect::<Vec<u16>>();
        let mut name_ptr = 0;
        let mut parts = Vec::new();
        while name_ptr < name_sequence.len() {
            // Copy the next 5+6+2 chars into a buffer to 0-pad them for LFN encoding
            let mut name_buffer = [0u16; 5 + 6 + 2];
            let space_left = min(name_buffer.len(), name_sequence.len() - name_ptr);
            name_buffer[0..space_left]
                .copy_from_slice(&name_sequence[name_ptr..name_ptr + space_left]);
            name_ptr += name_buffer.len();
            parts.push(name_buffer);
        }

        #[cfg(debug_assertions)]
        println!("Name sequence: {:?}", name_sequence);
        #[cfg(debug_assertions)]
        println!("Name parts: {:?}", parts);

        // Iterate backwards because LFN entries go in backwards order!
        for i in (0..parts.len()).rev() {
            #[cfg(debug_assertions)]
            println!("Adding parts[{}] to FS", i);
            // Determine the LFN sequence number based on the index in the sequence
            let sequence_number = if i == parts.len() - 1 {
                // 6th bit high means this is the first entry
                0x40 | (i + 1) as u8
            } else { (i + 1) as u8 };

            let mut name1 = [0u16; 5];
            name1.copy_from_slice(&parts[i][0..5]);
            let mut name2 = [0u16; 6];
            name2.copy_from_slice(&parts[i][5..11]);
            let mut name3 = [0u16; 2];
            name3.copy_from_slice(&parts[i][11..13]);

            let entries = [VFatLfnDirEntry {
                sequence_number,
                name1,
                attrs: Attributes::default().lfn(),
                kind: 0u8,
                checksum,
                name2,
                __r0: 0u16,
                name3,
            }];
            let buf = unsafe { &entries.cast::<u8>() };
            start = self.vfat.lock(|vfat: &mut VFat<HANDLE>| -> io::Result<Pos> {
                let amt_written = vfat.write_chain_pos(start, &buf)?;

                #[cfg(debug_assertions)]
                println!("Seeking {} bytes forward from {:?}", amt_written, start);

                vfat.seek_and_extend(start, amt_written)
            })?;

            #[cfg(debug_assertions)]
            println!("Ended up at {:?}", start);
        }
        self.create_entry(meta, start, parent)
    }
}

pub struct DirIter<HANDLE: VFatHandle> {
    vfat: HANDLE,
    entries: Vec<VFatDirEntry>,
    index: usize,
    parent: Cluster,
}

impl<HANDLE: VFatHandle> DirIter<HANDLE> {
    fn get_meta(&mut self) -> Option<(Metadata, Cluster)> {
        // Max 256 characters in filename but we add 13 chars at a time...
        // 260 is the smallest multiple of 13 that can fit 255 chars
        let mut name_sequence = [0u16; 260];
        let mut long_name = false;

        loop {
            #[cfg(debug_assertions)]
            println!("Trying to look for another entry ({})!", self.index);

            if self.index >= self.entries.len() {
                return None
            }
            let entry = &self.entries[self.index];
            self.index += 1;

            let unknown = unsafe { entry.unknown };

            if unknown.valid == 0x00 {
                #[cfg(debug_assertions)]
                println!("It's invalid");
                return None
            } else if unknown.valid == 0xE5 {
                #[cfg(debug_assertions)]
                println!("It's deleted");
                long_name = false;
                name_sequence = [0u16; 260];
                continue
            }

            let is_lfn = unknown.attrs.is_lfn();

            #[cfg(debug_assertions)]
            println!("Is it LFN? {}", is_lfn);

            if is_lfn {
                let lfn = unsafe { entry.long_filename };
                
                #[cfg(debug_assertions)]
                println!("Found checksum: {}", lfn.checksum);

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
        let (meta, cluster) = self.get_meta()?;
        let original_index = self.index - 1;
        let entry_size = core::mem::size_of::<VFatUnknownDirEntry>();
        let base_pos = Pos { cluster: self.parent, offset: 0 };
        let entry = self.vfat.lock(|vfat: &mut VFat<HANDLE>| -> Option<Pos> {
            vfat.seek(base_pos, original_index * entry_size).ok()
        });
        if meta.attributes.is_dir() {
            let dir = Dir {
                vfat: self.vfat.clone(),
                start: cluster,
                meta,
                entry,
            };
            Some(Entry::Dir(dir))
        } else {
            let file = File {
                vfat: self.vfat.clone(),
                start: cluster,
                meta,
                entry,
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

    /// The type of metadata for entries in this directory.
    type Metadata = crate::vfat::Metadata;

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
            parent: self.start,
        })
    }

    /// Creates a new entry in the directory.
    fn create(&mut self, meta: Self::Metadata) -> io::Result<Self::Entry> {
        // Find the first index where we can create a new entry
        let mut end_index = 0;
        let mut buf = Vec::new();
        self.vfat.lock(|vfat: &mut VFat<HANDLE>| -> io::Result<()> {
            vfat.read_chain(self.start, &mut buf)?;
            Ok(())
        })?;

        let entries = unsafe { buf.cast::<VFatUnknownDirEntry>() };
        while end_index < entries.len() && entries[end_index].valid != 0x00 {
            end_index += 1;
        }

        let start_pos =
            if end_index == 0 {
                Pos {
                    cluster: self.start,
                    offset: 0,
                }
            } else {
                let prev_pos = end_index;
                self.get_start_pos(prev_pos)?
            };
        #[cfg(debug_assertions)]
        println!("Start position: {:?}", start_pos);

        // Now determine whether the new entry is gonna be LFN or regular
        let name = meta.name.clone();
        let parts = name.split('.').collect::<Vec<_>>();
        let base_length = parts[0].len();
        let ext_length = if parts.len() > 1 {
            parts[1].len()
        } else {
            0
        };

        // Determine if we're root & if not pass the start for the parent dir
        let parent = match self.entry {
            Some(_) => Some(self.start),
            None => None,
        };
        if base_length > 8 || ext_length > 3 || parts.len() > 2 {
            self.create_lfn_entry(meta, start_pos, parent)
        } else {
            self.create_entry(meta, start_pos, parent)
        }
    }
}
