use core::alloc::Layout;
use core::fmt;
//use core::ptr;

use crate::allocator::linked_list::LinkedList;
use crate::allocator::util::*;
use crate::allocator::LocalAlloc;

const NUM_BINS: usize = 30;

/// A simple allocator that allocates based on size classes.
///   bin 0 (2^3 bytes)    : handles allocations in (0, 2^3]
///   bin 1 (2^4 bytes)    : handles allocations in (2^3, 2^4]
///   ...
///   bin 29 (2^22 bytes): handles allocations in (2^31, 2^32]
///   
///   map_to_bin(size) -> k
///   
pub struct Allocator {
    start: usize,
    end: usize,
    bins: [LinkedList; NUM_BINS],
}

struct BlockHeader {
    _next: LinkedList,
    size: usize,
    free: bool,
    padding: usize,
}

struct BlockFooter {
    size: usize,
    free: bool,
}

// Figure out ceil(log_2(n)) by repeatedly shifting till we get 0
fn map_to_bin(mut size: usize) -> usize {
    let mut k = 0;
    size -= 1; // avoid an off by one, since we start with 2^0 = 1
    while size > 0 {
        size >>= 1;
        k += 1;
    }
    if k >= 3 {
        k - 3 // bin 0 => 2^3
    } else {
        0 // default to 0
    }
}

impl Allocator {
    /// Creates a new bin allocator that will allocate memory from the region
    /// starting at address `start` and ending at address `end`.
    pub fn new(start: usize, end: usize) -> Allocator {
        let start = align_up(start, 8);
        let end = align_down(end, 8);
        let mem_size = end - start;
        let header_size = core::mem::size_of::<BlockHeader>();
        let footer_size = core::mem::size_of::<BlockFooter>();
        let meta_size = header_size + footer_size;
        unsafe {
            let main_block_ptr = start as *mut BlockHeader;
            *main_block_ptr = BlockHeader {
                _next: LinkedList::new(),
                size: mem_size - meta_size,
                free: true,
                padding: 0,
            };
            let end_block_ptr = (start + mem_size - footer_size) as *mut BlockFooter;
            *end_block_ptr = BlockFooter {
                size: mem_size - meta_size,
                free: false,
            };
        }
        let mut allocator = Allocator {
            start,
            end,
            bins: [LinkedList::new(); NUM_BINS]
        };
        unsafe {
            allocator.bins[NUM_BINS - 1].push(start as *mut usize);
        }
        allocator
    }

    unsafe fn merge_left(&mut self, block: *mut BlockHeader) -> *mut BlockHeader {
        let header_size = core::mem::size_of::<BlockHeader>();
        let footer_size = core::mem::size_of::<BlockFooter>();

        if (*block).padding != 0 { panic!("Can't merge padded block"); }
        let block_base = block as *mut u8;
        let prev_end = block_base.sub(footer_size);

        // If the prev block is out of bounds return early
        if prev_end as usize <= self.start { return block; }

        let prev_footer = prev_end as *mut BlockFooter;

        // Can't merge if the previous block isn't free
        if !(*prev_footer).free { return block; }

        // Assume that if free, a node's padding is always 0
        let prev_size = (*prev_footer).size;
        let prev_start = prev_end.sub(header_size + prev_size);
        let prev_header = prev_start as *mut BlockHeader;

        if (*prev_header).size != prev_size {
            return block;
        }

        // If the prev block is out of bounds return early
        if prev_start as usize <= self.start { return block; }

        // How do we pop this from the list if the size is now wrong?
        // Find the node's block & iterate thru till we find this node
        let old_bin = map_to_bin(prev_size);
        for node in self.bins[old_bin].iter_mut() {
            let iblock = node.value() as *mut BlockHeader;
            if iblock == prev_header {
                node.pop();
                break;
            }
        }

        let block_footer = block_base.add(header_size + (*block).size) as *mut BlockFooter;
        if (*block_footer).size != (*block).size {
            return block; //panic!("SIZE CORREUPED");
        }

        *prev_header = BlockHeader {
            _next: LinkedList::new(),
            size: prev_size + footer_size + header_size + (*block).size,
            padding: 0,
            free: true,
        };

        *block_footer = BlockFooter {
            size: (*prev_header).size,
            free: true,
        };

        prev_header
    }

    unsafe fn merge_right(&mut self, block: *mut BlockHeader) {
        let header_size = core::mem::size_of::<BlockHeader>();
        let footer_size = core::mem::size_of::<BlockFooter>();

        if (*block).padding != 0 { panic!("Can't merge padded block"); }
        let block_base = block as *mut u8;
        
        // Since we remove padding on deallocation, we can assume that if the next node
        // is free it'll have no padding. To check if this is true, we start by assuming
        // that the next node is free. We then cross-check its header & footer to see if
        // the sizes match between these. If so, we assume it's free.
        let next_start = block_base.add(header_size + (*block).size + footer_size);

        // If the next block is out of bounds return early
        if next_start as usize >= self.end { return; }

        let next_header = next_start as *mut BlockHeader;

        // Can't merge if the previous block isn't free
        if !(*next_header).free { return; }

        let next_size = (*next_header).size;
        let next_footer = next_start.add(header_size + next_size) as *mut BlockFooter;

        // If the next block is out of bounds return early
        if next_footer as *mut u8 as usize >= self.end { return; }

        if next_size != (*next_footer).size { return; }

        // How do we pop this from the list if the size is now wrong?
        // Find the node's block & iterate thru till we find this node
        let old_bin = map_to_bin(next_size);
        for node in self.bins[old_bin].iter_mut() {
            let iblock = node.value() as *mut BlockHeader;
            if iblock == next_header {
                node.pop();
                break;
            }
        }

        let block_size = (*block).size;
        *block = BlockHeader {
            _next: LinkedList::new(),
            size: block_size + footer_size + header_size + next_size,
            padding: 0,
            free: true,
        };

        *next_footer = BlockFooter {
            size: (*block).size,
            free: true,
        };
    }

    fn place_node(&mut self, block: *mut BlockHeader) {
        let block_size = unsafe { (*block).size };
        let bin_num = map_to_bin(block_size);
        unsafe {
            self.bins[bin_num].push(block as *mut usize);
        }
    }
}

impl LocalAlloc for Allocator {
    /// Allocates memory. Returns a pointer meeting the size and alignment
    /// properties of `layout.size()` and `layout.align()`.
    ///
    /// If this method returns an `Ok(addr)`, `addr` will be non-null address
    /// pointing to a block of storage suitable for holding an instance of
    /// `layout`. In particular, the block will be at least `layout.size()`
    /// bytes large and will be aligned to `layout.align()`. The returned block
    /// of storage may or may not have its contents initialized or zeroed.
    ///
    /// # Safety
    ///
    /// The _caller_ must ensure that `layout.size() > 0` and that
    /// `layout.align()` is a power of two. Parameters not meeting these
    /// conditions may result in undefined behavior.
    ///
    /// # Errors
    ///
    /// Returning null pointer (`core::ptr::null_mut`)
    /// indicates that either memory is exhausted
    /// or `layout` does not meet this allocator's
    /// size or alignment constraints.
    unsafe fn alloc(&mut self, layout: Layout) -> *mut u8 {
        // If we align size to 8, all splits will happen on 8-byte boundaries
        // If we *don't* do this, using the linked-list at a non-8-byte-boundary
        // will cause a CPU exception
        let request_size = align_up(layout.size(), 8);
        let bin_num = map_to_bin(request_size);
        for i in bin_num..NUM_BINS {
            if self.bins[i].is_empty() {
               continue; 
            }

            let header_size = core::mem::size_of::<BlockHeader>();
            let footer_size = core::mem::size_of::<BlockFooter>();

            let min_size = header_size + 8 + footer_size; // 2^3 bytes data
            let min_split_size = request_size + min_size; // need extra header to split
            for node in self.bins[i].iter_mut() {
                let block = node.value() as *mut BlockHeader;
                let block_base = block as *mut u8;
                let block_data_base = block_base as usize + header_size as usize;
                let block_size = (*block).size;
                let aligned_base = align_up(block_data_base, layout.align()) as *mut u8;
                let padding = aligned_base.sub(block_data_base) as usize;
                if padding > block_size { continue }
                let aligned_size = block_size - padding;
                let aligned_block = aligned_base.sub(header_size) as *mut BlockHeader;
                // if big enough, either take the entire node or split when possible
                if aligned_size >= request_size && aligned_size < min_split_size {
                    node.pop();
                    (*aligned_block).free = false;
                    (*aligned_block).padding = padding;
                    (*aligned_block).size = aligned_size;

                    let end_block = aligned_base.add(aligned_size) as *mut BlockFooter;
                    (*end_block).size = aligned_size;
                    (*end_block).free = false;
                    return aligned_base 
                } else if aligned_size >= min_split_size {
                    node.pop();

                    // [padding]
                    // [header]
                    // {data} [request_size]
                    // [footer]
                    // [padding = 0]
                    // [header]
                    // {data} [leftover]
                    // [footer]

                    let leftover = aligned_size - request_size - footer_size - header_size;

                    (*aligned_block).free = false;
                    (*aligned_block).padding = padding;
                    (*aligned_block).size = request_size;
                    
                    let og_size = header_size + block_size + footer_size;
                    let new_size = padding + header_size + request_size + footer_size + header_size + leftover + footer_size;
                    let actual_size = padding + header_size + (*aligned_block).size + footer_size + header_size + leftover + footer_size;
                    if og_size != new_size || og_size != actual_size {
                        panic!("Size OLD {} != NEW {}", og_size, new_size);
                    }

                    // Next block starts immediately after our footer
                    let split_base = aligned_base.add(request_size + footer_size);
                    let split_header = split_base as *mut BlockHeader;
                    *split_header = BlockHeader {
                        _next: LinkedList::new(),
                        size: leftover,
                        free: true,
                        padding: 0,
                    };

                    // Next block's footer is at the end of the leftover space
                    let split_footer = split_base.add(header_size + leftover) as *mut BlockFooter;
                    
                    if split_footer as usize >= self.end {
                        panic!("Block footer is too far, size was corrupt");
                    }
                    *split_footer = BlockFooter {
                        size: leftover,
                        free: true,
                    };

                    self.place_node(split_header);

                    let end_block = aligned_base.add((*aligned_block).size) as *mut BlockFooter;
                    (*end_block).size = (*aligned_block).size;
                    (*end_block).free = false;

                    return aligned_base
                }
            }
        }

        core::ptr::null_mut()
    }

    /// Deallocates the memory referenced by `ptr`.
    ///
    /// # Safety
    ///
    /// The _caller_ must ensure the following:
    ///
    ///   * `ptr` must denote a block of memory currently allocated via this
    ///     allocator
    ///   * `layout` must properly represent the original layout used in the
    ///     allocation call that returned `ptr`
    ///
    /// Parameters not meeting these conditions may result in undefined
    /// behavior.
    unsafe fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        let header_size = core::mem::size_of::<BlockHeader>();

        let header = ptr.sub(header_size) as *mut BlockHeader;
        let size = (*header).size;
        let padding = (*header).padding;
        let footer = ptr.add(size) as *mut BlockFooter;

        if size != (*footer).size {
            panic!("SIZE CORRUPTED {}, FOOTER {}, requested size {}", size, (*footer).size, layout.size());
        }

        let start = (header as *mut u8).sub(padding);
        let new_header = start as *mut BlockHeader;
        *new_header = BlockHeader {
            _next: LinkedList::new(),
            size: size + padding,
            padding: 0,
            free: true,
        };

        *footer = BlockFooter {
            size: (*new_header).size,
            free: true,
        };

        self.merge_right(new_header);
        let merged = self.merge_left(new_header);

        let footer = (merged as *mut u8).add(header_size + (*merged).size) as *mut BlockFooter;
        if (*merged).size != (*footer).size {
            panic!("Merging corrupted size -- merged {}, size {}, end size {}", merged as usize, (*merged).size, (*footer).size);
        }

        self.place_node(merged);
    }
}

impl fmt::Display for BlockHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.free {
            write!(f, "{}", self.size)
        } else {
            write!(f, "(allocated, padding: {}, size: {})", self.padding, self.size)
        }
    }
}

impl fmt::Debug for Allocator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{")?;
        for bin_num in 0..NUM_BINS {
            write!(f, "{}: [", bin_num)?;
            for node in self.bins[bin_num].iter() {
                let header = unsafe { &*(node as *mut BlockHeader) };
                write!(f, "{}, ", header)?;
            }
            write!(f, "]")?;
        }
        write!(f, "}}")
    }
}
