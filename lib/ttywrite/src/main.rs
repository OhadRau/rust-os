mod parsers;

use serial;
use structopt;
use structopt_derive::StructOpt;
use xmodem::{Xmodem, Progress};

use std::path::PathBuf;
use std::time::Duration;

use structopt::StructOpt;
use serial::core::{CharSize, BaudRate, StopBits, FlowControl, SerialDevice, SerialPortSettings};

use parsers::{parse_width, parse_stop_bits, parse_flow_control, parse_baud_rate};

#[derive(StructOpt, Debug)]
#[structopt(about = "Write to TTY using the XMODEM protocol by default.")]
struct Opt {
    #[structopt(short = "i", help = "Input file (defaults to stdin if not set)", parse(from_os_str))]
    input: Option<PathBuf>,

    #[structopt(short = "b", long = "baud", parse(try_from_str = "parse_baud_rate"),
                help = "Set baud rate", default_value = "115200")]
    baud_rate: BaudRate,

    #[structopt(short = "t", long = "timeout", parse(try_from_str),
                help = "Set timeout in seconds", default_value = "10")]
    timeout: u64,

    #[structopt(short = "w", long = "width", parse(try_from_str = "parse_width"),
                help = "Set data character width in bits", default_value = "8")]
    char_width: CharSize,

    #[structopt(help = "Path to TTY device", parse(from_os_str))]
    tty_path: PathBuf,

    #[structopt(short = "f", long = "flow-control", parse(try_from_str = "parse_flow_control"),
                help = "Enable flow control ('hardware' or 'software')", default_value = "none")]
    flow_control: FlowControl,

    #[structopt(short = "s", long = "stop-bits", parse(try_from_str = "parse_stop_bits"),
                help = "Set number of stop bits", default_value = "1")]
    stop_bits: StopBits,

    #[structopt(short = "r", long = "raw", help = "Disable XMODEM")]
    raw: bool,
}

fn main() {
    use std::fs::File;
    use std::io::{self, BufRead, BufReader};

    let opt = Opt::from_args();

    let mut port = serial::open(&opt.tty_path).expect("path points to invalid TTY");
    let mut serial_settings = port.read_settings().expect("serial settings not available");
    serial_settings.set_baud_rate(opt.baud_rate).expect("unable to set port baud rate");
    serial_settings.set_char_size(opt.char_width);
    serial_settings.set_stop_bits(opt.stop_bits);
    serial_settings.set_flow_control(opt.flow_control);
    port.write_settings(&serial_settings).expect("unable to write to port settings");
    port.set_timeout(Duration::new(opt.timeout, 0)).expect("unable to set port timeout");

    let target = opt.tty_path.to_str().unwrap();
    let mut input_buffer: Box<dyn BufRead> = match opt.input {
        None => {
            let stdin = io::stdin();
            Box::new(BufReader::new(stdin))
        },
        Some(name) => {
            let fd = File::open(name).expect("Unable to open file");
            Box::new(BufReader::new(fd))
        }
    };

    let num_written =
        if opt.raw {
            io::copy(&mut input_buffer, &mut port).expect("couldn't copy to port") as usize
        } else {
            fn print_progress(p: Progress) {
                println!("Progress: {:?}", p);
            }
            Xmodem::transmit_with_progress(input_buffer, &mut port, print_progress).expect("couldn't transmit")
        };
    println!("wrote {} bytes to {}", num_written, target);
}
