#![feature(asm)]
#![no_std]
#![no_main]

mod cr0;

use kernel_api::{print, println};
use kernel_api::syscall::{input, output, fork, exec, wait_pid, exit};

fn parse_command<'a>(buffer: &'a [u8], args_buf: &'a mut [&'a str]) -> Option<&'a [&'a str]> {
    let mut start = 0;
    let mut arg = 0;
    for i in 0..buffer.len() {
        if buffer[i] == ' ' as u8 || i == buffer.len() - 1 {
            let part = core::str::from_utf8(&buffer[start..i]).ok()?;
            args_buf[arg] = part;
            arg += 1;
            start = i + 1;
        }
    }
    Some(&args_buf[0..arg])
}

fn run_program(program: &str, args: &[&str]) {
    if program == "exit" { exit() }
    match fork() {
        Ok(0) => match exec(program, args) {
            Ok(()) => (),
            Err(e) => println!("Encountered error: {:?}", e),
        },
        Ok(pid) => match wait_pid(pid) {
            Ok(()) => (),
            Err(e) => println!("Failed to wait for process: {:?}", e),
        },
        Err(e) => println!("Error running while {}: {:?}", program, e),
    }
}

fn main(_args: &[&str]) {
    loop {
        print!("sh> ");

        let mut text_idx = 0;
        let mut text_buf = [0u8; 512];
        let mut args_buf = [""; 64];

        loop {
            if text_idx >= 512 { continue; }
            let ch = input();
            text_buf[text_idx] = ch;
            text_idx += 1;
            output(ch);
            if ch == '\n' as u8 || ch == '\r' as u8 {
                break;
            }
        }
        println!("");

        let command_text = &text_buf[0..text_idx];
        match parse_command(command_text, &mut args_buf) {
            Some(args) => {
                if args.len() == 0 { continue }
                let program = &args[0];
                let args = &args[1..];
                run_program(program, args);
            },
            None => println!("Parse error!"),
        }
    }
}
