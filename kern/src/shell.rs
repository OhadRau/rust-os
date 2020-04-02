use shim::io;
use shim::path::PathBuf;

use stack_vec::StackVec;

use pi::atags::Atags;

use fat32::traits::{Dir, Entry, FileSystem, Metadata};

use crate::console::{kprint, kprintln, CONSOLE};
use crate::FILESYSTEM;

/// Error type for `Command` parse failures.
#[derive(Debug)]
enum Error {
    Empty,
    TooManyArgs,
}

/// A structure representing a single shell command.
struct Command<'a> {
    args: StackVec<'a, &'a str>,
}

impl<'a> Command<'a> {
    /// Parse a command from a string `s` using `buf` as storage for the
    /// arguments.
    ///
    /// # Errors
    ///
    /// If `s` contains no arguments, returns `Error::Empty`. If there are more
    /// arguments than `buf` can hold, returns `Error::TooManyArgs`.
    fn parse(s: &'a str, buf: &'a mut [&'a str]) -> Result<Command<'a>, Error> {
        let mut args = StackVec::new(buf);
        for arg in s.split(' ').filter(|a| !a.is_empty()) {
            args.push(arg).map_err(|_| Error::TooManyArgs)?;
        }

        if args.is_empty() {
            return Err(Error::Empty);
        }

        Ok(Command { args })
    }

    /// Returns this command's path. This is equivalent to the first argument.
    fn path(&self) -> &str {
        self.args[0]
    }

    fn eval(&self, cwd: &mut PathBuf) {
        match self.path() {
            "echo" => {
                for arg in &self.args[1..] {
                    kprint!("{} ", arg);
                }
                kprintln!("");
            },
            "panic" => panic!("ARE YOU THE BRAIN SPECIALIST?"),
            "lsatag" => {
                for tag in Atags::get() {
                    kprintln!("{:#?}", tag)
                }
            },
            "memorymap" => {
                match crate::allocator::memory_map() {
                    Some((start, end)) =>
                        kprintln!("Memory available: [{}..{}]", start, end),
                    None => kprintln!("Couldn't load memory map")
                }
            },
            "testalloc" => {
                use alloc::vec::Vec;

                let mut v = Vec::new();
                for i in 0..50 {
                    v.push(i);
                    kprintln!("{:?}", v);
                }
            },
            "pwd" => pwd(cwd),
            "cd" => { 
                if self.args.len() > 1 {
                    cd(cwd, self.args[1]);
                }
            },
            "ls" => ls(cwd, &self.args[1..]),
            "cat" => cat(cwd, &self.args[1..]),
            path => kprintln!("unknown command: {}", path)
        }
    }
}

fn pwd(cwd: &mut PathBuf) {
    let path = cwd.as_path();
    let path_str = path.to_str().expect("Failed to get working directory");
    kprintln!("{}", path_str);
}

fn cd(cwd: &mut PathBuf, path: &str) -> bool {
    if path.len() == 0 { return true }
    if &path[0..1] == "/" {
        // cwd.clear() not implemented in shim :(
        while cwd.pop() {}
    }
    for part in path.split('/') {
        // Remove any / that makes its way in
        let part = part.replace("/", "");
        if part == "." {
            continue
        } else if part == ".." {
            cwd.pop();
        } else {
            cwd.push(&part);
            match FILESYSTEM.open(cwd.as_path()) {
                Ok(entry) => {
                    if entry.is_file() {
                        kprintln!("{}: Not a directory", part);
                        cwd.pop();
                        return false
                    }
                }
                Err(_) => {
                    kprintln!("{}: No such file or directory", part);
                    cwd.pop();
                    return false
                }
            }
        }
    }

    return true
}

fn ls(cwd: &PathBuf, args: &[&str]) {
    let mut rel_dir = cwd.clone();
    let mut changed_dir = false;
    let mut show_hidden = false;
    for arg in args {
        if *arg == "-a" {
            show_hidden = true;
            continue
        }

        if changed_dir {
            continue
        }

        if !cd(&mut rel_dir, arg) {
            return
        } else {
            changed_dir = true // only run cd once
        }
    }

    // no need to cd . if they didn't change dir

    let entry = FILESYSTEM.open(rel_dir.as_path()).expect("Couldn't open dir");
    let dir = entry.as_dir().expect("Expected directory, found file");
    for item in dir.entries().expect("Couldn't get a dir iterator") {
        if show_hidden || !item.metadata().hidden() {
            kprintln!("{}", item.metadata())
        }
    }
}

fn cat(cwd: &PathBuf, args: &[&str]) {
    fn cat_one(cwd: &PathBuf, path: &str) {
        use core::str;
        use io::Read;
        use alloc::vec::Vec;
        use alloc::slice::SliceConcatExt;

        let mut rel_dir = cwd.clone();

        let parts = path.split('/').collect::<Vec<&str>>();

        let dir = parts[0..parts.len()-1].join("/");
        if !cd(&mut rel_dir, &dir) {
            return
        }

        rel_dir.push(parts[parts.len()-1]);
        let entry = FILESYSTEM.open(rel_dir.as_path()).expect("Couldn't open file");
        if !entry.is_file() {
            kprintln!("Can't cat a directory {}!", path);
            return
        }
        let mut file = entry.into_file().expect("Expected file, found directory");
        
        loop {
            let mut buffer = [0u8; 256];
            match file.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    let string = str::from_utf8(&buffer[0..n]);
                    match string {
                        Ok(string) => kprint!("{}", string),
                        Err(_) => {
                            kprintln!("Couldn't parse {} as UTF-8", path);
                            return
                        },
                    }
                },
                Err(e) => {
                    kprintln!("Error when reading file {}: {:?}", path, e);
                    return
                }
            }
        }
    }

    for arg in args {
        cat_one(cwd, arg)
    }
}

/// Starts a shell using `prefix` as the prefix for each line. This function
/// never returns.
pub fn shell(prefix: &str) -> ! {
    use core::str;

    let mut path_buf = PathBuf::from("/");

    loop {
        let mut text_idx = 0;
        let mut text_buf = [0u8; 512];
        let mut args_buf = [""; 64];
        kprint!("{}{} ", path_buf.to_str().unwrap_or_default(), prefix);
        loop {
            let byte = CONSOLE.lock().read_byte();
            if byte == b'\n' || byte == b'\r' {
                break;
            } else if byte == 8 || byte == 127 { // backspace
                if text_idx > 0 {
                    text_idx -= 1;
                    text_buf[text_idx] = b' ';
                    kprint!("\x08 \x08");
                }
            } else if byte != b'\t' && (byte < 32 || byte > 127) { // invisible
                kprint!("\x07"); // ring bell
            } else if text_idx < text_buf.len() {
                text_buf[text_idx] = byte;
                text_idx += 1;
                kprint!("{}", byte as char);
            }
        }
        kprintln!("");
        let buf_str = str::from_utf8(&text_buf[..text_idx]).unwrap_or_default();
        match Command::parse(buf_str, &mut args_buf) {
            Err(Error::Empty) => (),
            Err(Error::TooManyArgs) => kprintln!("error: too many arguments"),
            Ok(cmd) => cmd.eval(&mut path_buf)
        }
    }
}
