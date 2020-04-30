mod frame;
mod syndrome;
mod syscall;

pub mod irq;
pub use self::frame::TrapFrame;

use pi::interrupt::{Controller, Interrupt};

use self::syndrome::{Syndrome, Fault};
use self::syscall::handle_syscall;
use crate::console::kprintln;

#[repr(u16)]
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum Kind {
    Synchronous = 0,
    Irq = 1,
    Fiq = 2,
    SError = 3,
}

#[repr(u16)]
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum Source {
    CurrentSpEl0 = 0,
    CurrentSpElx = 1,
    LowerAArch64 = 2,
    LowerAArch32 = 3,
}

#[repr(C)]
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub struct Info {
    source: Source,
    kind: Kind,
}

/// This function is called when an exception occurs. The `info` parameter
/// specifies the source and kind of exception that has occurred. The `esr` is
/// the value of the exception syndrome register. Finally, `tf` is a pointer to
/// the trap frame for the exception.
#[no_mangle]
pub extern "C" fn handle_exception(info: Info, esr: u32, tf: &mut TrapFrame) {
    //use crate::console::kprintln;
    //kprintln!("Handling exception! Source: {:?}, kind: {:?}, tf: {:?}, esr: {}",
              //info.source, info.kind, tf, esr);
    if info.kind == Kind::Synchronous {
        let syndrome = Syndrome::from(esr);
        let addr = unsafe { aarch64::FAR_EL1.get() } as usize;
        //kprintln!("Detected syndrome {:?} ({:b}, FAR = {:x})", syndrome, esr, addr);
        //kprintln!("Stack pointer: {:x}; Instruction addr: {:x}", tf.sp, tf.elr);
        //kprintln!("ttbr0: {:x}, ttbr1: {:x}", tf.ttbr0, tf.ttbr1);
        //let mut sp = 0;
        //unsafe { asm!("mov $0, sp" : "=r"(sp) ::::) }
        //kprintln!("SP: {:x}", sp);
        match syndrome {
            Syndrome::Brk(0xFFFF) => {
                // this is used to indicate a request from su command (kind of a hack)
                tf.elr += 4; 
                crate::shell::shell("#");
            }
            Syndrome::Brk(_) => {
                tf.elr += 4; // Go to next instruction
                kprintln!("brk elr: {}", tf.elr);
                crate::shell::shell(" [brk]>")
            },
            Syndrome::Svc(num) => handle_syscall(num, tf),
            Syndrome::DataAbort { kind: Fault::Translation, level: 3 } =>
                {
                    if !crate::SCHEDULER.with_running(move |p| p.page_fault(addr)).unwrap() {
                        crate::SCHEDULER.kill(tf);
                    }
                },
            Syndrome::InstructionAbort { kind: Fault::Translation, level: 3 } =>
                {
                    if !crate::SCHEDULER.with_running(move |p| p.page_fault(addr)).unwrap() {
                        crate::SCHEDULER.kill(tf);
                    }
                },
            _ => kprintln!("Detected syndrome {:?} ({:b}, FAR = {:x})", syndrome, esr, addr),
        }
    } else if info.kind == Kind::Irq {
        let controller = Controller::new();
        for int in Interrupt::iter() {
            if controller.is_pending(*int) {
                crate::IRQ.invoke(*int, tf);
            }
        }
    }
}
