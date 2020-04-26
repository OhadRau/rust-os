use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use hashbrown::HashMap;
use core::sync::atomic::AtomicBool;
use shim::io;
use shim::path::Path;

use aarch64;

use crate::param::*;
use crate::process::State;
use crate::traps::TrapFrame;
use crate::vm::*;
use crate::fs::fd::LocalFdTable;
use kernel_api::{OsError, OsResult};

/// Type alias for the type of a process ID.
pub type Id = u64;

/// A structure that represents the complete state of a process.
#[derive(Debug)]
pub struct Process {
    /// The saved trap frame of a process.
    pub context: Box<TrapFrame>,
    /// The page table describing the Virtual Memory of the process
    pub vmap: Box<UserPageTable>,
    /// The scheduling state of the process.
    pub state: State,
    /// Reference to tell us if the process has died
    pub dead: Arc<AtomicBool>,
    /// Table of available file descriptors
    pub fd_table: LocalFdTable,
    /// Environment variables
    pub env: HashMap<String, String>,
    /// Last allocated page (other than the stack)
    pub last_page: VirtualAddr,
}

impl Process {
    /// Creates a new process with a zeroed `TrapFrame` (the default), a zeroed
    /// stack of the default size, and a state of `Ready`.
    ///
    /// If enough memory could not be allocated to start the process, returns
    /// `None`. Otherwise returns `Some` of the new `Process`.
    pub fn new() -> OsResult<Process> {
        let tf = TrapFrame::default();
        let state = State::Ready;
        Ok(Process {
            context: Box::new(tf),
            vmap: Box::new(UserPageTable::new()),
            state,
            dead: Arc::new(AtomicBool::new(false)),
            fd_table: LocalFdTable::new(),
            env: HashMap::new(),
            last_page: VirtualAddr::from(0),
        })
    }

    pub fn fork(&self, tf: &TrapFrame) -> Process {
        Process {
            context: Box::new(tf.clone()),
            vmap: Box::new(self.vmap.duplicate()),
            state: State::Ready,
            dead: Arc::new(AtomicBool::new(false)),
            fd_table: self.fd_table.clone(),
            env: self.env.clone(),
            last_page: self.last_page.clone(),
        }
    }

    /// Load a program stored in the given path by calling `do_load()` method.
    /// Set trapframe `context` corresponding to the its page table.
    /// `sp` - the address of stack top
    /// `elr` - the address of image base.
    /// `ttbr0` - the base address of kernel page table
    /// `ttbr1` - the base address of user page table
    /// `spsr` - `F`, `A`, `D` bit should be set.
    ///
    /// Returns Os Error if do_load fails.
    pub fn load<P: AsRef<Path>>(pn: P) -> OsResult<Process> {
        use crate::VMM;

        let mut p = Process::do_load(pn)?;

        // Set trapframe for the process.
        p.context.sp = Self::get_stack_top().as_u64();
        p.context.elr = Self::get_image_base().as_u64();
        p.context.ttbr0 = VMM.get_baddr().as_u64();
        p.context.ttbr1 = p.vmap.get_baddr().as_u64();
        p.context.spsr |= aarch64::SPSR_EL1::F | aarch64::SPSR_EL1::A | aarch64::SPSR_EL1::D;

        Ok(p)
    }

    /// Creates a process and open a file with given path.
    /// Allocates one page for stack with read/write permission, and N pages with read/write/execute
    /// permission to load file's contents.
    fn do_load<P: AsRef<Path>>(pn: P) -> OsResult<Process> {
        use io::Read;
        use alloc::vec;
        use core::cmp::min;
        use crate::FILESYSTEM;
        use fat32::traits::{Entry, FileSystem};

        let mut process = Process::new()?;
        process.vmap.alloc(Self::get_stack_base(), PagePerm::RW);
        
        let entry = FILESYSTEM.open(pn)?;
        let file_size = entry.metadata().size;
        let mut file = match entry.into_file() {
            Some(file) => file,
            None => return Err(OsError::NoEntry),
        };
        let mut pos = 0;
        let mut buffer = vec![0u8; file_size];
        loop {
            match file.read(&mut buffer[pos..])? {
                0 => break,
                n => pos += n
            }
        }

        let num_pages = 1 + (file_size / PAGE_SIZE);
        for i in 0..num_pages {
            let base = VirtualAddr::from(USER_IMG_BASE + i * PAGE_SIZE);
            let page = process.vmap.alloc(base, PagePerm::RWX);
            process.last_page = base;
            let num_bytes = min(PAGE_SIZE, file_size - i * PAGE_SIZE);
            page[0..num_bytes].copy_from_slice(&buffer[0..num_bytes]);
        }
        
        Ok(process)
    }

    /// Load a program to an existing process
    pub fn load_existing<P: AsRef<Path>>(&mut self, pn: P) -> OsResult<()> {
        use io::Read;
        use alloc::vec;
        use core::cmp::min;
        use crate::{VMM, FILESYSTEM};
        use fat32::traits::{Entry, FileSystem};

        self.vmap.try_alloc(Self::get_stack_base(), PagePerm::RW);

        let entry = FILESYSTEM.open(pn)?;
        let file_size = entry.metadata().size;
        let mut file = match entry.into_file() {
            Some(file) => file,
            None => return Err(OsError::NoEntry),
        };
        let mut pos = 0;
        let mut buffer = vec![0u8; file_size];
        loop {
            match file.read(&mut buffer[pos..])? {
                0 => break,
                n => pos += n
            }
        }

        let num_pages = 1 + (file_size / PAGE_SIZE);
        for i in 0..num_pages {
            let base = VirtualAddr::from(USER_IMG_BASE + i * PAGE_SIZE);
            let page = self.vmap.try_alloc(base, PagePerm::RWX);
            self.last_page = base;
            let num_bytes = min(PAGE_SIZE, file_size - i * PAGE_SIZE);
            page[0..num_bytes].copy_from_slice(&buffer[0..num_bytes]);
        }

        // Set trapframe for the process.
        self.context.sp = Self::get_stack_top().as_u64();
        self.context.elr = Self::get_image_base().as_u64();
        self.context.ttbr0 = VMM.get_baddr().as_u64();
        self.context.ttbr1 = self.vmap.get_baddr().as_u64();
        self.context.spsr |= aarch64::SPSR_EL1::F | aarch64::SPSR_EL1::A | aarch64::SPSR_EL1::D;

        Ok(())
    }

    /// Pass args to an existing program
    pub fn init_args(&mut self, mut args: &[&str]) {
        /* What needs to happen? Given an array of arguments:
           1. Allocate a big buffer
           2. Copy each string into the buffer & store the offset for each string
           3. Create a vector that stores all the pointers to strings + their lengths
              a) This is hard because we need to maintain u64 alignment (8 bytes)
              b) We also need to calculate the final location of each pointer
                 (i.e. from the bottom of the stack). This is easier if we can know
                 the final size of the buffer before creating it.
        */
        use kernel_api::ARG_MAX;
        use alloc::vec;
        use fat32::util::SliceExt;
        use crate::allocator::util::{align_up, align_down};

        let fat_pointer_size = core::mem::size_of::<(usize, *const u8)>();
        let buffer_array_size = fat_pointer_size * args.len();
        let mut buffer_size = buffer_array_size;

        if args.len() > ARG_MAX {
            crate::kprintln!("[WARNING]: Attempted to pass too many args to program. Only the first {} will be passed.", ARG_MAX);
            args = &args[0..ARG_MAX];
        } else if args.len() == 0 {
            self.context.xs[0] = 0u64;
            self.context.xs[1] = 0u64;
            return;
        }

        for arg in args {
            buffer_size += arg.len();
        }

        let buffer_start_addr = align_down(Self::get_stack_top().as_usize() - buffer_size, PAGE_SIZE);

        let mut buffer = vec![(0, core::ptr::null()); align_up(buffer_size, PAGE_SIZE) / fat_pointer_size];
        let mut buf_addr = buffer_array_size;

        for i in 0..args.len() {
            let len = args[i].len();
            let bytes = args[i].as_bytes();
            buffer[i] = (len, (buffer_start_addr + buf_addr) as *const u8);

            let char_buffer = unsafe { &mut buffer.cast_mut::<u8>() };
            char_buffer[buf_addr..buf_addr+len].copy_from_slice(&bytes[0..len]);
            buf_addr += len;
        }

        let mut base_addr = buffer_start_addr;
        while base_addr < Self::get_stack_top().as_usize() {
            let va = VirtualAddr::from(base_addr);
            let page = self.vmap.try_alloc(va, PagePerm::RW);

            let idx = base_addr - buffer_start_addr;
            let char_buffer = unsafe { &mut buffer.cast_mut::<u8>() };
            page.copy_from_slice(&char_buffer[idx..idx.saturating_add(PAGE_SIZE)]);

            base_addr = base_addr.saturating_add(PAGE_SIZE);
        }

        let stack_va = VirtualAddr::from(buffer_start_addr - PAGE_SIZE);
        let _ = self.vmap.try_alloc(stack_va, PagePerm::RW);

        self.context.sp -= buffer_size as u64;
        self.context.xs[0] = args.len() as u64;
        self.context.xs[1] = buffer_start_addr as u64;
    }

    pub fn page_fault(&mut self, addr: usize) -> bool {
        let page_addr = addr % PAGE_SIZE;
        if page_addr >= USER_STACK_START {
            self.vmap.alloc(VirtualAddr::from(addr), PagePerm::RW);
            true
        } else {
            false
        }
    }

    /// Returns the highest `VirtualAddr` that is supported by this system.
    pub fn get_max_va() -> VirtualAddr {
        VirtualAddr::from(USER_IMG_BASE + USER_MAX_VM_SIZE)
    }

    /// Returns the `VirtualAddr` represents the base address of the user
    /// memory space.
    pub fn get_image_base() -> VirtualAddr {
        VirtualAddr::from(USER_IMG_BASE)
    }

    /// Returns the `VirtualAddr` represents the base address of the user
    /// process's stack.
    pub fn get_stack_base() -> VirtualAddr {
        VirtualAddr::from(USER_STACK_BASE)
    }

    /// Returns the `VirtualAddr` represents the top of the user process's
    /// stack.
    pub fn get_stack_top() -> VirtualAddr {
        use crate::allocator::util::align_down;
        // Won't let me add max_va :(
        VirtualAddr::from(align_down(usize::max_value(), 16))
    }

    /// Returns `true` if this process is ready to be scheduled.
    ///
    /// This functions returns `true` only if one of the following holds:
    ///
    ///   * The state is currently `Ready`.
    ///
    ///   * An event being waited for has arrived.
    ///
    ///     If the process is currently waiting, the corresponding event
    ///     function is polled to determine if the event being waiting for has
    ///     occured. If it has, the state is switched to `Ready` and this
    ///     function returns `true`.
    ///
    /// Returns `false` in all other cases.
    pub fn is_ready(&mut self) -> bool {
        match self.state {
            State::Ready => true,
            State::Waiting(_) => {
                let old_state = core::mem::replace(&mut self.state, State::Ready);
                if let State::Waiting(mut poll_fn) = old_state {
                    let ready = poll_fn(self);
                    if !ready {
                        core::mem::replace(&mut self.state, State::Waiting(poll_fn));
                    }
                    ready
                } else {
                    panic!("State changed from waiting to something else")
                }
            },
            _ => false
        }
    }
}
