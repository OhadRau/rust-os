use alloc::boxed::Box;
use alloc::collections::vec_deque::VecDeque;

use aarch64::*;

use crate::mutex::Mutex;
use crate::param::TICK;
use crate::process::{Id, Process, State};
use crate::traps::TrapFrame;

/*
extern fn start_shell() {
    use crate::shell;

    loop { shell::shell(" [proc]>"); }
}
*/

/// Process scheduler for the entire machine.
#[derive(Debug)]
pub struct GlobalScheduler(Mutex<Option<Scheduler>>);

impl GlobalScheduler {
    /// Returns an uninitialized wrapper around a local scheduler.
    pub const fn uninitialized() -> GlobalScheduler {
        GlobalScheduler(Mutex::new(None))
    }

    /// Enter a critical region and execute the provided closure with the
    /// internal scheduler.
    pub fn critical<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut Scheduler) -> R,
    {
        let mut guard = self.0.lock();
        f(guard.as_mut().expect("scheduler uninitialized"))
    }


    /// Adds a process to the scheduler's queue and returns that process's ID.
    /// For more details, see the documentation on `Scheduler::add()`.
    pub fn add(&self, process: Process) -> Option<Id> {
        self.critical(move |scheduler| scheduler.add(process))
    }

    /// Performs a context switch using `tf` by setting the state of the current
    /// process to `new_state`, saving `tf` into the current process, and
    /// restoring the next process's trap frame into `tf`. For more details, see
    /// the documentation on `Scheduler::schedule_out()` and `Scheduler::switch_to()`.
    pub fn switch(&self, new_state: State, tf: &mut TrapFrame) -> Id {
        self.critical(|scheduler| scheduler.schedule_out(new_state, tf));
        self.switch_to(tf)
    }

    pub fn switch_to(&self, tf: &mut TrapFrame) -> Id {
        loop {
            let rtn = self.critical(|scheduler| scheduler.switch_to(tf));
            if let Some(id) = rtn {
                return id;
            }
            wfi();
        }
    }

    /// Kills currently running process and returns that process's ID.
    /// For more details, see the documentaion on `Scheduler::kill()`.
    #[must_use]
    pub fn kill(&self, tf: &mut TrapFrame) -> Option<Id> {
        self.critical(|scheduler| scheduler.kill(tf))
    }
    
    /// Starts executing processes in user space using timer interrupt based
    /// preemptive scheduling. This method should not return under normal conditions.
    pub fn start(&self) -> ! {
        use pi::timer;
        use pi::interrupt::{Controller, Interrupt};
   
        Controller::new().enable(Interrupt::Timer1);
        timer::tick_in(TICK);

        let mut trap_frame = TrapFrame::default();
        self.switch_to(&mut trap_frame);
        let tf = &trap_frame as *const TrapFrame as u64;

        unsafe {
            asm!("
                // Call context_restore w/ SP reset to trap frame
                mov sp, $0
                bl context_restore
                mov lr, xzr

                // Move SP to next page w/out clobbering other registers
                adr x0, _start_next_page
                mov sp, x0
                mov x0, xzr

                // Switch to EL0
                eret

_start_next_page:
                .balign 0x10000
            " :: "r"(tf) :: "volatile")
        }

        loop {}
    }

    /// Initializes the scheduler and add userspace processes to the Scheduler
    pub unsafe fn initialize(&self) {
        use pi::timer;
        use pi::interrupt::Interrupt;

        *self.0.lock() = Some(Scheduler::new());

        crate::IRQ.register(Interrupt::Timer1, Box::new(|tf: &mut TrapFrame| {
            timer::tick_in(TICK);
            crate::SCHEDULER.switch(State::Ready, tf);
        }));

        let a = Process::load("/sleep").expect("couldn't load sleep");
        let b = Process::load("/fib").expect("couldn't load fib");
        let c = Process::load("/sleep").expect("couldn't load sleep");
        let d = Process::load("/fib").expect("couldn't load fib");
        self.add(a).expect("Couldn't get PID");
        self.add(b).expect("Couldn't get PID");
        self.add(c).expect("Couldn't get PID");
        self.add(d).expect("Couldn't get PID");
    }

    // The following method may be useful for testing Phase 3:
    //
    // * A method to load a extern function to the user process's page table.
    //
    /*
    pub fn test_phase_3(&self, proc: &mut Process){
        use crate::vm::{VirtualAddr, PagePerm};

        let mut page = proc.vmap.alloc(
            VirtualAddr::from(USER_IMG_BASE as u64), PagePerm::RWX);
    
        let text = unsafe {
            core::slice::from_raw_parts(test_user_process as *const u8, 24)
        };
    
        page[0..24].copy_from_slice(text);
    }
    */
}

#[derive(Debug)]
pub struct Scheduler {
    processes: VecDeque<Process>,
    last_id: Option<Id>,
}

impl Scheduler {
    /// Returns a new `Scheduler` with an empty queue.
    fn new() -> Scheduler {
        Scheduler {
            processes: VecDeque::new(),
            last_id: None,
        }
    }

    /// Adds a process to the scheduler's queue and returns that process's ID if
    /// a new process can be scheduled. The process ID is newly allocated for
    /// the process and saved in its `trap_frame`. If no further processes can
    /// be scheduled, returns `None`.
    ///
    /// It is the caller's responsibility to ensure that the first time `switch`
    /// is called, that process is executing on the CPU.
    fn add(&mut self, mut process: Process) -> Option<Id> {
        let pid = self.last_id.and_then(|x| x.checked_add(1)).unwrap_or_default();

        process.context.tpidr = pid;
        self.processes.push_back(process);

        self.last_id = Some(pid);
        self.last_id
    }

    /// Finds the currently running process, sets the current process's state
    /// to `new_state`, prepares the context switch on `tf` by saving `tf`
    /// into the current process, and push the current process back to the
    /// end of `processes` queue.
    ///
    /// If the `processes` queue is empty or there is no current process,
    /// returns `false`. Otherwise, returns `true`.
    fn schedule_out(&mut self, new_state: State, tf: &mut TrapFrame) -> bool {
        let process = self.processes.pop_front();
        match process {
            Some(mut p @ Process { state: State::Running, .. }) => {
                p.state = new_state;
                p.context = Box::new(*tf);
                self.processes.push_back(p);
                true
            },
            _ => false,
        }
    }

    /// Finds the next process to switch to, brings the next process to the
    /// front of the `processes` queue, changes the next process's state to
    /// `Running`, and performs context switch by restoring the next process`s
    /// trap frame into `tf`.
    ///
    /// If there is no process to switch to, returns `None`. Otherwise, returns
    /// `Some` of the next process`s process ID.
    fn switch_to(&mut self, tf: &mut TrapFrame) -> Option<Id> {
        // Find the first ready process
        let index = self.processes.iter_mut().position(|item: &mut Process| -> bool {
            item.is_ready()
        })?;
        // Remove it from the queue
        let mut next_process = self.processes.remove(index)?;
        let pid = next_process.context.tpidr;
        
        // Set it to running & restore its context to the target trap frame
        next_process.state = State::Running;
        *tf = *next_process.context;
        // Push it to the front of the queue
        self.processes.push_front(next_process);
        // Return the process ID
        Some(pid)
    }

    /// Kills currently running process by scheduling out the current process
    /// as `Dead` state. Removes the dead process from the queue, drop the
    /// dead process's instance, and returns the dead process's process ID.
    fn kill(&mut self, tf: &mut TrapFrame) -> Option<Id> {
        if self.schedule_out(State::Dead, tf) {
            let killed = self.processes.pop_back()?;
            let pid = killed.context.tpidr;
            core::mem::drop(killed); // Force dropping the instance NOW
            self.switch_to(tf);
            Some(pid)
        } else {
            None
        }
    }
}

/*
pub extern "C" fn  test_user_process() -> ! {
    loop {
        let ms = 10000;
        let error: u64;
        let elapsed_ms: u64;

        unsafe {
            asm!("mov x0, $2
              svc 1
              mov $0, x0
              mov $1, x7"
                 : "=r"(elapsed_ms), "=r"(error)
                 : "r"(ms)
                 : "x0", "x7"
                 : "volatile");
        }
    }
}
*/
