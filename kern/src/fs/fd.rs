use hashbrown::{HashMap, HashSet};

use fat32::vfat::Entry;
use fat32::traits::FileSystem as FS;

use shim::path::PathBuf;
use shim::{io, ioerr, newioerr};

use crate::FILESYSTEM;
use crate::mutex::Mutex;
use crate::fs::PiVFatHandle;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Fd(u64);

impl Fd {
  pub fn as_u64(&self) -> u64 {
    self.0
  }
}

impl core::convert::From<u64> for Fd {
  fn from(d: u64) -> Self {
    Fd(d)
  }
}

#[derive(Debug)]
pub struct FdTable {
  next_free: u64,
  map: HashMap<Fd, (PathBuf, Entry<PiVFatHandle>, usize)>,
  busy_paths: HashSet<PathBuf>,
}

impl FdTable {
  fn new() -> FdTable {
    FdTable {
      next_free: 0,
      map: HashMap::new(),
      busy_paths: HashSet::new(),
    }
  }

  pub fn open(&mut self, path: PathBuf) -> io::Result<Fd> {
    if self.busy_paths.contains(&path) {
      return ioerr!(PermissionDenied, "That file is already in use by another process")
    }

    let key = Fd(self.next_free);
    let entry = FILESYSTEM.open(&path)?;
    self.map.insert(key, (path.clone(), entry, 1));
    self.busy_paths.insert(path);
    // Hopefully this is never an issue, but theoretically we could overflow:
    self.next_free += 1;
    Ok(key)
  }

  pub fn duplicate(&mut self, fd: &Fd) -> io::Result<()> {
    let (_, _, ref_count) =
      self.map.get_mut(fd).ok_or(newioerr!(NotFound, "No such fd open"))?;
    *ref_count += 1;
    Ok(())
  }

  pub fn close(&mut self, fd: &Fd) -> io::Result<()> {
    let (_, _, ref_count) =
      self.map.get_mut(fd).ok_or(newioerr!(NotFound, "No such fd open"))?;
    if *ref_count == 1 {
      let (path, _, _) = self.map.remove(fd).ok_or(newioerr!(NotFound, "No such fd open"))?;
      self.busy_paths.remove(&path);
    } else {
      *ref_count -= 1;
    }
    Ok(())
  }

  pub fn get(&self, fd: &Fd) -> io::Result<&Entry<PiVFatHandle>> {
    let (_, entry, _) =
      self.map.get(fd).ok_or(newioerr!(NotFound, "No such fd open"))?;
    Ok(entry)
  }

  pub fn get_mut(&mut self, fd: &Fd) -> io::Result<&mut Entry<PiVFatHandle>> {
    let (_, entry, _) =
      self.map.get_mut(fd).ok_or(newioerr!(NotFound, "No such fd open"))?;
    Ok(entry)
  }
}

pub struct GlobalFdTable(Mutex<Option<FdTable>>);

impl GlobalFdTable {
  pub const fn uninitialized() -> Self {
    GlobalFdTable(Mutex::new(None))
  }

  pub fn initialize(&self) {
    let mut guard = self.0.lock();

    match *guard {
      Some(_) => panic!("Attempted to initialize two global file descriptor tables"),
      None => (),
    }

    *guard = Some(FdTable::new())
  }

  pub fn critical<F, R>(&self, f: F) -> R
  where
    F: FnOnce(&mut FdTable) -> R,
  {
    let mut guard = self.0.lock();
    f(guard.as_mut().expect("global FD table uninitialized"))
  }
}

#[derive(Debug)]
pub struct LocalFdTable(HashSet<Fd>);

impl Clone for LocalFdTable {
  fn clone(&self) -> Self {
    for fd in self.0.iter() {
      let _ = crate::FILE_DESCRIPTOR_TABLE.critical(move |table: &mut FdTable| {
        table.duplicate(fd)
      });
    }
    LocalFdTable(self.0.clone())
  }
}

impl Drop for LocalFdTable {
  fn drop(&mut self) {
    for fd in self.0.iter() {
      let _ = crate::FILE_DESCRIPTOR_TABLE.critical(move |table: &mut FdTable| {
        table.close(fd)
      });
    }
  }
}

impl LocalFdTable {
  pub fn new() -> Self {
    LocalFdTable(HashSet::new())
  }

  pub fn open(&mut self, path: PathBuf) -> io::Result<Fd> {
    let fd = crate::FILE_DESCRIPTOR_TABLE.critical(move |table: &mut FdTable| -> io::Result<Fd> {
      table.open(path)
    })?;
    self.0.insert(fd);
    Ok(fd)
  }

  pub fn close(&mut self, fd: &Fd) -> io::Result<()> {
    crate::FILE_DESCRIPTOR_TABLE.critical(move |table: &mut FdTable| {
      table.close(fd)
    })?;
    self.0.remove(fd);
    Ok(())
  }

  pub fn critical<F, R>(&self, fd: &Fd, f: F) -> io::Result<R>
  where
    F: FnOnce(&mut Entry<PiVFatHandle>) -> R,
  {
    if self.0.contains(fd) {
      crate::FILE_DESCRIPTOR_TABLE.critical(move |table: &mut FdTable| -> io::Result<R> {
        Ok(f(table.get_mut(fd)?))
      })
    } else {
      ioerr!(NotFound, "Fd must be owned by this process")
    }
  }
}
