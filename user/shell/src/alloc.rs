use core::alloc::{GlobalAlloc, Layout};
use core::cell::Cell;

use kernel_api::syscall::request_page;

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

pub struct Allocator(Cell<Option<BumpAllocator>>);

impl Allocator {
  pub const fn uninitialized() -> Self {
      Allocator(Cell::new(None))
  }

  pub unsafe fn initialize(&self) {
      self.0.set(Some(BumpAllocator::new()));
  }
}

unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut local = self.0.take().expect("Allocator not initialized");
        let result = local.alloc(layout);
        self.0.set(Some(local));
        result
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let mut local = self.0.take().expect("Allocator not initialized");
        local.dealloc(ptr, layout);
        self.0.set(Some(local));
    }
}

pub const ALLOCATOR: Allocator = Allocator::uninitialized();
