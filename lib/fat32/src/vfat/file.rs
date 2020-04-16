use shim::io::{self, SeekFrom};
use shim::ioerr;
use crate::traits;
use crate::vfat::{Cluster, Dir, Metadata, VFat, VFatHandle, Pos, Range};
use crate::vfat::dir::VFatRegularDirEntry;
use core::mem;
    

#[derive(Debug)]
pub struct File<HANDLE: VFatHandle> {
    pub vfat: HANDLE,
    pub start: Cluster,
    pub meta: Metadata,
    pub entry: Option<Range>,
    pub pos: Pos,
    pub amt_read: usize,
}

impl<HANDLE: VFatHandle> File<HANDLE> {
    // updates the regular file entry for this file to match the current metadata
    // does not account for lfn entries
    pub fn update_entry(&self) -> io::Result<usize> {
        let reg_entry = self.into();
        let reg_entry_buf: &[u8] = unsafe { &mem::transmute::<VFatRegularDirEntry, [u8; 32]>(reg_entry) };
        match self.entry {
            Some(Range { end: e, .. }) => {
                self.vfat.lock(|vfat: &mut VFat<HANDLE>| -> io::Result<usize> {
                    vfat.write_cluster(e.cluster, e.offset, reg_entry_buf)
                })
            },
            _ => ioerr!(NotFound, "file entry not found")
        }
    }
}

impl<HANDLE: VFatHandle> io::Read for File<HANDLE> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        use shim::io::Seek;
        use core::cmp::min;

        let num_bytes = min(buf.len(), self.meta.size - self.amt_read);
        if num_bytes == 0 {
            return Ok(0)
        }

        let num_read = self.vfat.lock(|vfat: &mut VFat<HANDLE>| -> io::Result<usize> {
            // Limit to num_bytes: we don't wanna read more than the
            // whole file length even if there's a chain that goes
            // longer than that.
            vfat.read_chain_pos(self.pos, &mut buf[0..num_bytes])
        })?;
        
        self.seek(SeekFrom::Current(num_read as i64))?;

        Ok(num_read)
    }
}

impl<HANDLE: VFatHandle> io::Write for File<HANDLE> {
    // writes buf into file starting from current pos in file
    // returns amount of data from buf that was actually written
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        use shim::io::Seek;

        if self.start.num() == 0 { // File is empty
            self.start = self.vfat.lock(|vfat: &mut VFat<HANDLE>| -> io::Result<Cluster> {
                vfat.alloc_cluster(crate::vfat::Status::Eoc(0))
                    .ok_or(io::Error::new(io::ErrorKind::AddrInUse, "Couldn't find free cluster"))
            })?;
        }

        let start_pos = Pos { cluster: self.start.clone(), offset: 0 };
        self.pos = self.vfat.lock(|vfat: &mut VFat<HANDLE>| -> io::Result<Pos> {
            vfat.seek_and_extend(start_pos, self.amt_read)
        })?;
            
        let bytes_written = self.vfat.lock(|vfat: &mut VFat<HANDLE>| -> io::Result<usize> {
            vfat.write_chain_pos(self.pos, buf)
        })?;

        // update the file size if necessary
        if self.meta.size < self.amt_read + bytes_written {
            self.meta.size = self.amt_read + bytes_written;
            self.update_entry()?;
        }

        // update current pos in file
        self.seek(SeekFrom::Current(bytes_written as i64)).expect("couldn't seek in file write"); 
        Ok(bytes_written)
    }

    fn flush(&mut self) -> io::Result<()> {
        // just flush the entire cached partition here
        // would be better to only flush sectors that pertain to this file
        // but that might not be possible with current implementation
        self.vfat.lock(|vfat: &mut VFat<HANDLE>| {
            vfat.flush();
        });
        Ok(())
    }
}

// FIXME: Implement `traits::File` (and its supertraits) for `File`.
impl<HANDLE: VFatHandle> traits::File for File<HANDLE> {
    /// Writes any buffered data to disk.
    fn sync(&mut self) -> io::Result<()> {
        use shim::io::Write;
        self.flush()
    }

    /// Returns the size of the file in bytes.
    fn size(&self) -> u64 {
        self.meta.size as u64
    }

    fn delete(&mut self) -> io::Result<()> {
        let entries_start = match self.entry {
            Some(Range {start, ..}) => start,
            None => return ioerr!(NotFound, "Cannot delete a file without a directory entry"),
        };
        self.vfat.lock(|vfat: &mut VFat<HANDLE>| -> io::Result<()> {
            // Free all the allocated space for the file's contents
            if self.size() > 0 { vfat.free_chain(self.start)?; }
            // Then mark all the dir entries as invalid
            Dir::invalidate_entries(vfat, entries_start)
        })
    }
}

impl<HANDLE: VFatHandle> io::Seek for File<HANDLE> {
    /// Seek to offset `pos` in the file.
    ///
    /// A seek to the end of the file is allowed. A seek _beyond_ the end of the
    /// file returns an `InvalidInput` error.
    ///
    /// If the seek operation completes successfully, this method returns the
    /// new position from the start of the stream. That position can be used
    /// later with SeekFrom::Start.
    ///
    /// # Errors
    ///
    /// Seeking before the start of a file or beyond the end of the file results
    /// in an `InvalidInput` error.
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let base = Pos {
            cluster: self.start,
            offset: 0,
        };

        let current = self.amt_read;
        
        let new = match pos {
            SeekFrom::Start(bytes) => bytes as i64,
            SeekFrom::Current(bytes) => current as i64 + bytes,
            SeekFrom::End(bytes) => self.meta.size as i64 + bytes,
        };

        if new < 0 || new > self.meta.size as i64 {
            return ioerr!(InvalidInput, "Can't seek past end of file")
        }

        self.amt_read = new as usize;

        self.pos = self.vfat.lock(|vfat: &mut VFat<HANDLE>| -> io::Result<Pos> {
            vfat.seek(base, new as usize)
        })?;

        Ok(new as u64)
    }
}
