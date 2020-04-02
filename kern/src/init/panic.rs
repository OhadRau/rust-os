use core::panic::PanicInfo;
use crate::console::kprintln;

const ERROR_ASCII_ART: &str = include_str!("error_art.txt");

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    kprintln!("{}", ERROR_ASCII_ART);
    kprintln!("------------------------PANIC------------------------");

    match info.location() {
        Some(location) => {
            kprintln!("FILE: {}", location.file());
            kprintln!("LINE: {}", location.line());
            kprintln!("COL:  {}", location.column());
        },
        None => ()
    }

    match info.message() {
        Some(msg) => kprintln!("\n{}", *msg),
        None => ()
    }
    loop {}
}
