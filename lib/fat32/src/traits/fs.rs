use shim::{io, ioerr, path::Path};
use crate::traits::Metadata;

/// Trait implemented by files in the file system.
pub trait File: io::Read + io::Write + io::Seek + Sized {
    /// Writes any buffered data to disk.
    fn sync(&mut self) -> io::Result<()>;

    /// Returns the size of the file in bytes.
    fn size(&self) -> u64;

    /// Delete the file & its contents from the filesystem
    fn delete(&mut self) -> io::Result<()>;
}

/// Trait implemented by directories in a file system.
pub trait Dir: Sized {
    /// The type of entry stored in this directory.
    type Entry: Entry;

    /// An type that is an iterator over the entries in this directory.
    type Iter: Iterator<Item = Self::Entry>;

    /// The type of metadata for entries in this directory.
    type Metadata: Metadata;

    /// Returns an interator over the entries in this directory.
    fn entries(&self) -> io::Result<Self::Iter>;
    /// Creates a new entry in the directory.
    fn create(&mut self, meta: Self::Metadata) -> io::Result<Self::Entry>;
    /// Delete the dir & its contents from the filesystem
    fn delete(&mut self) -> io::Result<()>;
}

/// Trait implemented by directory entries in a file system.
///
/// An entry is either a `File` or a `Directory` and is associated with both
/// `Metadata` and a name.
pub trait Entry: Sized {
    type File: File;
    type Dir: Dir;
    type Metadata: Metadata;

    /// The name of the file or directory corresponding to this entry.
    fn name(&self) -> &str;

    /// The metadata associated with the entry.
    fn metadata(&self) -> &Self::Metadata;

    /// If `self` is a file, returns `Some` of a reference to the file.
    /// Otherwise returns `None`.
    fn as_file(&self) -> Option<&Self::File>;

    /// If `self` is a directory, returns `Some` of a reference to the
    /// directory. Otherwise returns `None`.
    fn as_dir(&self) -> Option<&Self::Dir>;

    /// If `self` is a file, returns `Some` of the file. Otherwise returns
    /// `None`.
    fn into_file(self) -> Option<Self::File>;

    /// If `self` is a directory, returns `Some` of the directory. Otherwise
    /// returns `None`.
    fn into_dir(self) -> Option<Self::Dir>;

    /// Returns `true` if this entry is a file or `false` otherwise.
    fn is_file(&self) -> bool {
        self.as_file().is_some()
    }

    /// Returns `true` if this entry is a directory or `false` otherwise.
    fn is_dir(&self) -> bool {
        self.as_dir().is_some()
    }
}

/// Trait implemented by file systems.
pub trait FileSystem: Sized {
    /// The type of files in this file system.
    type File: File;

    /// The type of directories in this file system.
    type Dir: Dir<Entry = Self::Entry, Metadata = Self::Metadata>;

    /// The type of directory entries in this file system.
    type Entry: Entry<File = Self::File, Dir = Self::Dir>;

    type Metadata: Metadata;

    /// Opens the entry at `path`. `path` must be absolute.
    ///
    /// # Errors
    ///
    /// If `path` is not absolute, an error kind of `InvalidInput` is returned.
    ///
    /// If any component but the last in `path` does not refer to an existing
    /// directory, an error kind of `InvalidInput` is returned.
    ///
    /// If there is no entry at `path`, an error kind of `NotFound` is returned.
    ///
    /// All other error values are implementation defined.
    fn open<P: AsRef<Path>>(self, path: P) -> io::Result<Self::Entry>;

    // flush to disk
    fn flush(self);

    /// Opens the file at `path`. `path` must be absolute.
    ///
    /// # Errors
    ///
    /// In addition to the error conditions for `open()`, this method returns an
    /// error kind of `Other` if the entry at `path` is not a regular file.
    fn open_file<P: AsRef<Path>>(self, path: P) -> io::Result<Self::File> {
        self.open(path)?
            .into_file()
            .ok_or(io::Error::new(io::ErrorKind::Other, "not a regular file"))
    }

    /// Opens the directory at `path`. `path` must be absolute.
    ///
    /// # Errors
    ///
    /// In addition to the error conditions for `open()`, this method returns an
    /// error kind of `Other` if the entry at `path` is not a directory.
    fn open_dir<P: AsRef<Path>>(self, path: P) -> io::Result<Self::Dir> {
        self.open(path)?
            .into_dir()
            .ok_or(io::Error::new(io::ErrorKind::Other, "not a directory"))
    }
    
    fn create_dir<P: AsRef<Path>>(self, parent: P, metadata: Self::Metadata) -> io::Result<Self::Entry> {
        match self.open(parent)?.into_dir() {
            Some(mut dir) => {
                dir.create(metadata)
            }
            None => ioerr!(NotFound, "could not find parent directory")
        }
    }
}
