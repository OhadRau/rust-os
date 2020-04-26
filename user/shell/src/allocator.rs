use core::fmt;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use core::cell::UnsafeCell;
use core::ops::{DerefMut, Deref, Drop};
use core::alloc::{GlobalAlloc, Layout};

use kernel_api::syscall::request_page;

#[repr(align(32))]
pub struct Mutex<T> {
    data: UnsafeCell<T>,
    lock: AtomicBool,
    owner: AtomicUsize
}

unsafe impl<T: Send> Send for Mutex<T> { }
unsafe impl<T: Send> Sync for Mutex<T> { }

pub struct MutexGuard<'a, T: 'a> {
    lock: &'a Mutex<T>
}

impl<'a, T> !Send for MutexGuard<'a, T> { }
unsafe impl<'a, T: Sync> Sync for MutexGuard<'a, T> { }

impl<T> Mutex<T> {
    pub const fn new(val: T) -> Mutex<T> {
        Mutex {
            lock: AtomicBool::new(false),
            owner: AtomicUsize::new(usize::max_value()),
            data: UnsafeCell::new(val)
        }
    }
}

impl<T> Mutex<T> {
    // Once MMU/cache is enabled, do the right thing here. For now, we don't
    // need any real synchronization.
    pub fn try_lock(&self) -> Option<MutexGuard<T>> {
        let this = 0;
        if !self.lock.load(Ordering::Relaxed) || self.owner.load(Ordering::Relaxed) == this {
            self.lock.store(true, Ordering::Relaxed);
            self.owner.store(this, Ordering::Relaxed);
            Some(MutexGuard { lock: &self })
        } else {
            None
        }
    }

    // Once MMU/cache is enabled, do the right thing here. For now, we don't
    // need any real synchronization.
    #[inline(never)]
    pub fn lock(&self) -> MutexGuard<T> {
        // Wait until we can "aquire" the lock, then "acquire" it.
        loop {
            match self.try_lock() {
                Some(guard) => return guard,
                None => continue
            }
        }
    }

    fn unlock(&self) {
        self.lock.store(false, Ordering::Relaxed);
    }
}

impl<'a, T: 'a> Deref for MutexGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { & *self.lock.data.get() }
    }
}

impl<'a, T: 'a> DerefMut for MutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<'a, T: 'a> Drop for MutexGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.unlock()
    }
}

impl<T: fmt::Debug> fmt::Debug for Mutex<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.try_lock() {
            Some(guard) => f.debug_struct("Mutex").field("data", &&*guard).finish(),
            None => f.debug_struct("Mutex").field("data", &"<locked>").finish()
        }
    }
}

pub fn align_up(addr: usize, align: usize) -> usize {
  if !align.is_power_of_two() {
    panic!("align_down: alignment must be a power of 2")
  }
  let leftover = addr % align;
  if leftover == 0 {
    addr
  } else {
    addr.checked_add(align - leftover).unwrap()
  }
}

/// A "bump" allocator: allocates memory by bumping a pointer; never frees.
#[derive(Debug)]
pub struct BumpAllocator {
    current: usize,
    end: usize,
}

impl BumpAllocator {
    #[allow(dead_code)]
    pub fn new() -> BumpAllocator {
        let heap_start = request_page(0).expect("Couldn't get heap start");
        BumpAllocator {
            current: heap_start,
            end: heap_start,
        }
    }
}

pub trait LocalAlloc {
  unsafe fn alloc(&mut self, layout: Layout) -> *mut u8;
  unsafe fn dealloc(&mut self, ptr: *mut u8, layout: Layout);
}

impl LocalAlloc for BumpAllocator {
    unsafe fn alloc(&mut self, layout: Layout) -> *mut u8 {
        let curr_aligned = align_up(self.current, layout.align());
        if curr_aligned.saturating_add(layout.size()) >= self.end {
            self.end = request_page(1).expect("Couldn't request page");
        }
        let ptr = curr_aligned as *mut u8;
        self.current = curr_aligned.saturating_add(layout.size() + 1);
        ptr
    }

    unsafe fn dealloc(&mut self, _ptr: *mut u8, _layout: Layout) {
        // LEAK
    }
}

pub struct Allocator(Mutex<Option<BumpAllocator>>);

impl Allocator {
  pub const fn uninitialized() -> Self {
      Allocator(Mutex::new(None))
  }

  pub unsafe fn initialize(&self) {
      *self.0.lock() = Some(BumpAllocator::new());
  }
}

unsafe impl GlobalAlloc for Allocator {
  unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
      self.0
          .lock()
          .as_mut()
          .expect("allocator uninitialized")
          .alloc(layout)
  }

  unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
      self.0
          .lock()
          .as_mut()
          .expect("allocator uninitialized")
          .dealloc(ptr, layout);
  }
}

#[alloc_error_handler]
pub fn oom(_layout: Layout) -> ! {
    panic!("OOM");
}
