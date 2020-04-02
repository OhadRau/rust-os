use crate::traits;
use crate::vfat::{Dir, File, Metadata, VFatHandle};
use crate::vfat::dir;
use crate::vfat::file;

// You can change this definition if you want
#[derive(Debug)]
pub enum Entry<HANDLE: VFatHandle> {
    File(File<HANDLE>),
    Dir(Dir<HANDLE>),
}

impl<HANDLE: VFatHandle> traits::Entry for Entry<HANDLE> {
    type File = file::File<HANDLE>;
    type Dir = dir::Dir<HANDLE>;
    type Metadata = Metadata;

    /// The name of the file or directory corresponding to this entry.
    fn name(&self) -> &str {
        match self {
            Entry::File(file) => &file.meta.name,
            Entry::Dir(dir) => &dir.meta.name,
        }
    }

    /// The metadata associated with the entry.
    fn metadata(&self) -> &Self::Metadata {
        match self {
            Entry::File(file) => &file.meta,
            Entry::Dir(dir) => &dir.meta,
        }
    }

    /// If `self` is a file, returns `Some` of a reference to the file.
    /// Otherwise returns `None`.
    fn as_file(&self) -> Option<&file::File<HANDLE>> {
        match self {
            Entry::File(file) => Some(&file),
            Entry::Dir(_) => None,
        }
    }

    /// If `self` is a directory, returns `Some` of a reference to the
    /// directory. Otherwise returns `None`.
    fn as_dir(&self) -> Option<&dir::Dir<HANDLE>> {
        match self {
            Entry::File(_) => None,
            Entry::Dir(dir) => Some(&dir),
        }
    }

    /// If `self` is a file, returns `Some` of the file. Otherwise returns
    /// `None`.
    fn into_file(self) -> Option<file::File<HANDLE>> {
        match self {
            Entry::File(file) => Some(file),
            Entry::Dir(_) => None,
        }
    }

    /// If `self` is a directory, returns `Some` of the directory. Otherwise
    /// returns `None`.
    fn into_dir(self) -> Option<dir::Dir<HANDLE>> {
        match self {
            Entry::File(_) => None,
            Entry::Dir(dir) => Some(dir),
        }
    }
}
