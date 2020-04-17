use core::iter::Chain;
use core::ops::{Deref, DerefMut};
use core::slice::Iter;

use alloc::boxed::Box;
use alloc::fmt;
use core::alloc::{GlobalAlloc, Layout};

use crate::allocator;
use crate::param::*;
use crate::vm::{PhysicalAddr, VirtualAddr};
use crate::ALLOCATOR;

use aarch64::vmsa::*;
use shim::const_assert_size;

#[repr(C)]
pub struct Page([u8; PAGE_SIZE]);
const_assert_size!(Page, PAGE_SIZE);

impl Page {
    pub const SIZE: usize = PAGE_SIZE;
    pub const ALIGN: usize = PAGE_SIZE;

    fn layout() -> Layout {
        unsafe { Layout::from_size_align_unchecked(Self::SIZE, Self::ALIGN) }
    }
}

#[repr(C)]
#[repr(align(65536))]
pub struct L2PageTable {
    pub entries: [RawL2Entry; 8192],
}
const_assert_size!(L2PageTable, PAGE_SIZE);

impl L2PageTable {
    /// Returns a new `L2PageTable`
    fn new() -> L2PageTable {
        L2PageTable {
            entries: [RawL2Entry::new(0); 8192]
        }
    }

    /// Returns a `PhysicalAddr` of the pagetable.
    pub fn as_ptr(&self) -> PhysicalAddr {
        let addr = self as *const L2PageTable;
        PhysicalAddr::from(addr)
    }
}

#[derive(Copy, Clone)]
pub struct L3Entry(RawL3Entry);

impl L3Entry {
    /// Returns a new `L3Entry`.
    fn new() -> L3Entry {
        let entry = RawL3Entry::new(0);
        L3Entry(entry)
    }

    /// Returns `true` if the L3Entry is valid and `false` otherwise.
    fn is_valid(&self) -> bool {
        self.0.get_masked(RawL3Entry::VALID) == RawL3Entry::VALID
    }

    /// Extracts `ADDR` field of the L3Entry and returns as a `PhysicalAddr`
    /// if valid. Otherwise, return `None`.
    fn get_page_addr(&self) -> Option<PhysicalAddr> {
        if self.is_valid() {
            let addr = self.0.get_value(RawL3Entry::ADDR) as usize;
            Some(PhysicalAddr::from(addr * PAGE_SIZE))
        } else {
            None
        }
    }
}

#[repr(C)]
#[repr(align(65536))]
pub struct L3PageTable {
    pub entries: [L3Entry; 8192],
}
const_assert_size!(L3PageTable, PAGE_SIZE);

impl L3PageTable {
    /// Returns a new `L3PageTable`.
    fn new() -> L3PageTable {
        L3PageTable {
            entries: [L3Entry::new(); 8192]
        }
    }

    /// Returns a `PhysicalAddr` of the pagetable.
    pub fn as_ptr(&self) -> PhysicalAddr {
        let addr = self as *const L3PageTable;
        PhysicalAddr::from(addr)
    }
}

#[repr(C)]
#[repr(align(65536))]
pub struct PageTable {
    pub l2: L2PageTable,
    pub l3: [L3PageTable; 2],
}

impl PageTable {
    /// Returns a new `Box` containing `PageTable`.
    /// Entries in L2PageTable should be initialized properly before return.
    fn new(perm: u64) -> Box<PageTable> {
        let mut pt = Box::new(PageTable {
            l2: L2PageTable::new(),
            l3: [L3PageTable::new(), L3PageTable::new()]
        });

        for i in 0..pt.l3.len() {
            pt.l2.entries[i].set_value(perm, RawL2Entry::AP);
            pt.l2.entries[i].set_value(EntryType::Table, RawL2Entry::TYPE);
            pt.l2.entries[i].set_value(EntryValid::Valid, RawL2Entry::VALID);
            pt.l2.entries[i].set_value(pt.l3[i].as_ptr().as_u64() >> 16, RawL2Entry::ADDR);
            pt.l2.entries[i].set_value(1, RawL2Entry::AF);
        }

        pt
    }

    /// Returns the (L2index, L3index) extracted from the given virtual address.
    /// Since we are only supporting 1GB virtual memory in this system, L2index
    /// should be smaller than 2.
    ///
    /// # Panics
    ///
    /// Panics if the virtual address is not properly aligned to page size.
    /// Panics if extracted L2index exceeds the number of L3PageTable.
    fn locate(va: VirtualAddr) -> (usize, usize) {
        let addr = va.as_usize();
        if addr % PAGE_SIZE != 0 {
            panic!("Virtual address must be aligned to page size");
        }

        let l2index = (addr >> 29) & 0x1FFF;
        let l3index = (addr >> 16) & 0x1FFF;

        if l2index >= 2 {
            crate::console::kprintln!("{:x} -> {:x} -> {:x}", addr, addr >> 29, l2index);
            panic!("Virtual address {:x} L2 index exceeds # of L3 tables", va.as_u64());
        }
    
        (l2index, l3index)
    }

    /// Returns `true` if the L3entry indicated by the given virtual address is valid.
    /// Otherwise, `false` is returned.
    pub fn is_valid(&self, va: VirtualAddr) -> bool {
        let (l2index, l3index) = Self::locate(va);
        self.l3[l2index].entries[l3index].is_valid()
    }

    /// Returns `true` if the L3entry indicated by the given virtual address is invalid.
    /// Otherwise, `true` is returned.
    pub fn is_invalid(&self, va: VirtualAddr) -> bool {
        !self.is_valid(va)
    }

    /// Set the given RawL3Entry `entry` to the L3Entry indicated by the given virtual
    /// address.
    pub fn get_entry(&self, va: VirtualAddr) -> L3Entry {
        let (l2index, l3index) = Self::locate(va);
        self.l3[l2index].entries[l3index]
    }

    /// Set the given RawL3Entry `entry` to the L3Entry indicated by the given virtual
    /// address.
    pub fn set_entry(&mut self, va: VirtualAddr, entry: RawL3Entry) -> &mut Self {
        let (l2index, l3index) = Self::locate(va);
        self.l3[l2index].entries[l3index] = L3Entry(entry);
        self
    }

    /// Returns a base address of the pagetable. The returned `PhysicalAddr` value
    /// will point the start address of the L2PageTable.
    pub fn get_baddr(&self) -> PhysicalAddr {
        self.l2.as_ptr()
    }
}

// Implement `IntoIterator` for `&PageTable`.
impl<'a> IntoIterator for &'a PageTable {
    type Item = &'a L3Entry;
    type IntoIter = Chain<Iter<'a, L3Entry>, Iter<'a, L3Entry>>;

    fn into_iter(self) -> Self::IntoIter {
        self.l3[0].entries.iter().chain(self.l3[1].entries.iter())
    }
}

pub struct KernPageTable(Box<PageTable>);

impl KernPageTable {
    /// Returns a new `KernPageTable`. `KernPageTable` should have a `Pagetable`
    /// created with `KERN_RW` permission.
    ///
    /// Set L3entry of ARM physical address starting at 0x00000000 for RAM and
    /// physical address range from `IO_BASE` to `IO_BASE_END` for peripherals.
    /// Each L3 entry should have correct value for lower attributes[10:0] as well
    /// as address[47:16]. Refer to the definition of `RawL3Entry` in `vmsa.rs` for
    /// more details.
    pub fn new() -> KernPageTable {
        let mut pt = PageTable::new(EntryPerm::KERN_RW);
        let mem_start = 0x0000_0000;
        let (_, mem_end) = allocator::memory_map().expect("Couldn't get memory map");

        let mut mem_idx = mem_start;

        while mem_idx < mem_end {
            let mut entry = RawL3Entry::new(0);
            
            entry.set_value(EntryValid::Valid, RawL3Entry::VALID);
            entry.set_value(PageType::Page, RawL3Entry::TYPE);
            entry.set_value(EntryPerm::KERN_RW, RawL3Entry::AP);
            entry.set_value(1, RawL3Entry::AF);
            entry.set_value(EntryAttr::Mem, RawL3Entry::ATTR);
            entry.set_value(EntrySh::ISh, RawL3Entry::SH);

            let addr = mem_idx >> 16;
            entry.set_value(addr as u64, RawL3Entry::ADDR);

            pt.set_entry(VirtualAddr::from(mem_idx), entry);
            mem_idx += PAGE_SIZE;
        }

        mem_idx = IO_BASE;
        
        while mem_idx < IO_BASE_END {
            let mut entry = RawL3Entry::new(0);
            
            entry.set_value(EntryValid::Valid, RawL3Entry::VALID);
            entry.set_value(PageType::Page, RawL3Entry::TYPE);
            entry.set_value(EntryPerm::KERN_RW, RawL3Entry::AP);
            entry.set_value(1, RawL3Entry::AF);
            entry.set_value(EntryAttr::Dev, RawL3Entry::ATTR);
            entry.set_value(EntrySh::OSh, RawL3Entry::SH);

            let addr = mem_idx >> 16;
            entry.set_value(addr as u64, RawL3Entry::ADDR);

            pt.set_entry(VirtualAddr::from(mem_idx), entry);
            mem_idx += PAGE_SIZE;
        }

        KernPageTable(pt)
    }
}

pub enum PagePerm {
    RW,
    RO,
    RWX,
}

pub struct UserPageTable(Box<PageTable>);

impl UserPageTable {
    /// Returns a new `UserPageTable` containing a `PageTable` created with
    /// `USER_RW` permission.
    pub fn new() -> UserPageTable {
        let pt = PageTable::new(EntryPerm::USER_RW);
        
        /*let mut free_pages = LinkedList::new();
        
        let mut mem_idx = VM_START;
        while mem_idx < VM_END {
            free_pages.push(mem_idx);
        }*/

        UserPageTable(pt)
    }


    pub fn duplicate(&self) -> Self {
        let mut pt = Self::new();

        let mut mem_idx = USER_IMG_BASE;

        for entry in self.0.into_iter() {
            let va = VirtualAddr::from(mem_idx);
            if entry.is_valid() {
                let addr = entry.get_page_addr().expect("Couldn't get page table entry's ADDR field");
                let buf = pt.alloc(va, PagePerm::RWX);
                let contents = unsafe { core::slice::from_raw_parts(addr.as_ptr(), PAGE_SIZE) };
                buf.copy_from_slice(&contents);
            }
            mem_idx += PAGE_SIZE;
        }

        pt
    }

    pub fn try_alloc(&mut self, va: VirtualAddr, perm: PagePerm) -> &mut [u8] {
        if va.as_usize() < USER_IMG_BASE {
            panic!("Virtual address not in user range");
        }

        let va_local = va - VirtualAddr::from(USER_IMG_BASE);

        if self.0.is_valid(va_local) {
            let entry = self.0.get_entry(va_local);
            let mut addr = entry.get_page_addr().expect("Couldn't get page table entry's ADDR field");
            unsafe { core::slice::from_raw_parts_mut(addr.as_mut_ptr(), PAGE_SIZE) }
        } else {
            self.alloc(va, perm)
        }
    }

    /// Allocates a page and set an L3 entry translates given virtual address to the
    /// physical address of the allocated page. Returns the allocated page.
    ///
    /// # Panics
    /// Panics if the virtual address is lower than `USER_IMG_BASE`.
    /// Panics if the virtual address has already been allocated.
    /// Panics if allocator fails to allocate a page.
    ///
    /// TODO. use Result<T> and make it failurable
    /// TODO. use perm properly
    pub fn alloc(&mut self, va: VirtualAddr, _perm: PagePerm) -> &mut [u8] {
        if va.as_usize() < USER_IMG_BASE {
            panic!("Virtual address not in user range");
        } 
        
        let va = va - VirtualAddr::from(USER_IMG_BASE);
        
        if self.0.is_valid(va) {
            panic!("Can't allocate the same page twice");
        }

        let frame = unsafe { ALLOCATOR.alloc(Page::layout()) };
        if frame.is_null() {
            panic!("Could not allocate new page");
        }
        
        let mut entry = RawL3Entry::new(0);
        entry.set_value(PageType::Page, RawL3Entry::TYPE);
        entry.set_value(EntryValid::Valid, RawL3Entry::VALID);
        entry.set_value(EntrySh::ISh, RawL3Entry::SH);
        entry.set_value(EntryPerm::USER_RW, RawL3Entry::AP);
        entry.set_value(EntryAttr::Mem, RawL3Entry::ATTR);
        entry.set_value(1, RawL3Entry::AF);
        entry.set_value((frame as u64) >> 16, RawL3Entry::ADDR);
        self.0.set_entry(va, entry);

        unsafe { core::slice::from_raw_parts_mut(frame, PAGE_SIZE) }
    }
}

// Implement `Drop` for `UserPageTable`.
impl Drop for UserPageTable {
    fn drop(&mut self) {
        for page in self.0.into_iter() {
            if !page.is_valid() { continue }
            let mut addr = page.get_page_addr().expect("couldn't get page addr");
            let ptr = addr.as_mut_ptr();
            unsafe { ALLOCATOR.dealloc(ptr, Page::layout()) }
        }
    }
}

// Implement `fmt::Debug` as you need.
impl fmt::Debug for UserPageTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "UserPageTable {{")?;
        for (idx, page) in self.0.into_iter().enumerate() {
            if !page.is_valid() { continue }
            let va = idx << 16;
            let pa = page.get_page_addr().expect("couldn't get page addr");
            write!(f, "{} => {}, ", va, pa.as_usize())?;
        }
        write!(f, "}}")
    }
}

impl fmt::Debug for KernPageTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "KernPageTable {{")?;
        for (idx, page) in self.0.into_iter().enumerate() {
            if !page.is_valid() { continue }
            let va = idx << 16;
            let pa = page.get_page_addr().expect("couldn't get page addr");
            write!(f, "{} => {}, ", va, pa.as_usize())?;
        }
        write!(f, "}}")
    }
}

impl Deref for KernPageTable {
    type Target = PageTable;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Deref for UserPageTable {
    type Target = PageTable;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for KernPageTable {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl DerefMut for UserPageTable {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
