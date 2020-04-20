#![feature(asm)]
#![no_std]
#![no_main]

mod cr0;

use kernel_api::{print, println, EntryKind};
use kernel_api::syscall::{input, output, env_get, env_set, fork, fs_create, fs_open, fs_close, fs_delete, exec, wait_pid, exit};

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
    let _ = env_set("CWD", "/").expect("Couldn't set $CWD");
    let mut cwd_buf = [0u8; 128];
    let cwd_string = unsafe { core::str::from_utf8_unchecked_mut(&mut cwd_buf) };
    match env_get("CWD", cwd_string) {
        Ok(len) => println!("$CWD: {:?}", &cwd_string[0..len]),
        Err(e)  => println!("Couldn't read $CWD: {:?}", e),
    };

    match fs_open("/fib") {
        Ok(fd) => println!("/fib: {:?}", fd),
        Err(e) => println!("Couldn't open /fib: {:?}", e),
    }

    match fs_open("/fib") {
        Ok(fd) => println!("/fib: {:?}", fd),
        Err(e) => println!("Couldn't open /fib: {:?}", e),
    }

    match fs_open("/echo") {
        Ok(fd) => { println!("/echo: {:?}", fd); fs_close(&fd); },
        Err(e) => println!("Couldn't open /echo: {:?}", e),
    }

    match fs_create("/foo", EntryKind::Dir) {
        Ok(_)  => println!("Created /foo"),
        Err(e) => println!("Couldn't create /foo: {:?}", e),
    }

    match fs_open("/foo") {
        Ok(fd) => { println!("/foo: {:?}", fd); fs_close(&fd); },
        Err(e) => println!("Couldn't open /foo: {:?}", e),
    }

    match fs_delete("/foo") {
        Ok(_)  => println!("Deleted /foo"),
        Err(e) => println!("Couldn't delete /foo: {:?}", e),
    }

    match fs_open("/foo") {
        Ok(fd) => println!("/foo: {:?}", fd),
        Err(e) => println!("Couldn't open /foo: {:?}", e),
    }

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
