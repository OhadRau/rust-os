mod frame;
mod syndrome;
mod syscall;

pub mod irq;
pub use self::frame::TrapFrame;

use pi::interrupt::{Controller, Interrupt};

use self::syndrome::Syndrome;
use self::syscall::handle_syscall;

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
//    use crate::console::kprintln;

//    kprintln!("Handling exception! Source: {:?}, kind: {:?}, tf: {:?}, esr: {}",
//              info.source, info.kind, tf, esr);
    
    if info.kind == Kind::Synchronous {
        let syndrome = Syndrome::from(esr);
//        kprintln!("Detected syndrome {:?}", syndrome);
        match syndrome {
            Syndrome::Brk(_) => {
                tf.elr += 4; // Go to next instruction
                crate::shell::shell(" [brk]>")
            },
            Syndrome::Svc(num) => handle_syscall(num, tf),
            _ => (),
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
