use core::mem::zeroed;
use core::panic::PanicInfo;
use core::ptr::write_volatile;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    kernel_api::println!("PANICKED: {:?}", info);
    loop {}
}

unsafe fn zeros_bss() {
    extern "C" {
        static mut __bss_beg: u64;
        static mut __bss_end: u64;
    }

    let mut iter: *mut u64 = &mut __bss_beg;
    let end: *mut u64 = &mut __bss_end;

    while iter < end {
        write_volatile(iter, zeroed());
        iter = iter.add(1);
    }
}

#[no_mangle]
pub unsafe extern "C" fn _start(argc: usize, argv: *const (usize, *const u8)) -> ! {
    use kernel_api::ARG_MAX;

    zeros_bss();

    crate::ALLOCATOR.initialize();

    if argc > ARG_MAX { panic!("Exceeded max number of args {}", ARG_MAX) };
    let mut args = [""; ARG_MAX];

    for i in 0..argc {
        let (len, ptr) = *argv.offset(i as isize);
        let string = core::slice::from_raw_parts(ptr, len);
        args[i] = core::str::from_utf8(string).expect("Couldn't parse args as UTF-8");
    }

    crate::main(&args[0..argc]);
    kernel_api::syscall::exit();
}
