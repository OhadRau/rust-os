use shim::io::{self, SeekFrom};

use crate::traits;
use crate::vfat::{Cluster, Metadata, VFat, VFatHandle, Pos};

#[derive(Debug)]
pub struct File<HANDLE: VFatHandle> {
    pub vfat: HANDLE,
    pub start: Cluster,
    pub meta: Metadata,
    pub entry: Option<Pos>,
    pub pos: Pos,
    pub amt_read: usize,
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
    fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
        unimplemented!("FAT32 is not yet writeable")
    }

    fn flush(&mut self) -> io::Result<()> {
        unimplemented!("FAT32 is not yet writeable")
    }
}

// FIXME: Implement `traits::File` (and its supertraits) for `File`.
impl<HANDLE: VFatHandle> traits::File for File<HANDLE> {
    /// Writes any buffered data to disk.
    fn sync(&mut self) -> io::Result<()> {
        unimplemented!("FAT32 is not yet writeable")
    }

    /// Returns the size of the file in bytes.
    fn size(&self) -> u64 {
        self.meta.size as u64
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
        use shim::ioerr;

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
