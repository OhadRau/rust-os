use shim::io;
use shim::path::{PathBuf, Component};
use alloc::string::String;

use stack_vec::StackVec;

use pi::atags::Atags;

use fat32::traits::{Dir, Entry, FileSystem, Metadata, BlockDevice};
use fat32::vfat::BiosParameterBlock;
use fat32::mbr::MasterBootRecord;
use blockdev::mount::*;
use aes128::edevice::EncryptedDevice;

use alloc::vec::Vec;

use sd::sd::Sd;

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
                let mut v = Vec::new();
                for i in 0..50 {
                    v.push(i);
                    kprintln!("{:?}", v);
                }
            },
            "sleep" => {
                use core::time::Duration;
                if self.args.len() > 1 {
                    let span = match self.args[1].parse() {
                        Ok(span) => span,
                        Err(_) => {
                            kprintln!("Couldn't parse time");
                            return
                        }
                    };
                    let slept = kernel_api::syscall::sleep(Duration::from_millis(span));
                    match slept {
                        Ok(time) => kprintln!("Slept {:?}", time),
                        Err(e) => kprintln!("Couldn't sleep: {:?}", e),
                    }
                } else {
                    kprintln!("Must pass in # of millis to sleep");
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
            "mkdir" => mkdir(cwd, &self.args[1..]),
            "write_file_test" => write_file_test(cwd),
            "touch" => touch(cwd, &self.args[1..]),
            "rm" => rm(cwd, &self.args[1..]),
            "append" => append(cwd, &self.args[1..]),
            "lsblk" => FILESYSTEM.lsblk(),
            "mount" => mount(cwd, &self.args[1..]),
            "umount" => umount(cwd, &self.args[1]),
            "mkcrypt" => encrypt_part(&self.args[1..]),
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

fn canonicalize(path: PathBuf) -> Result<PathBuf, ()> {
    let mut new_path = PathBuf::new();

    for comp in path.components() {
        match comp {
            Component::ParentDir => {
                let res = new_path.pop();
                if !res {
                    return Err(());
                }
            },
            Component::Normal(n) => new_path = new_path.join(n),
            Component::RootDir => new_path = ["/"].iter().collect(),
            _ => ()
        };
    }

    Ok(new_path)
}

fn get_abs_path(cwd: &PathBuf, dir_arg: &str) -> Option<PathBuf> {
    let mut raw_path: PathBuf = PathBuf::from(dir_arg);
    if !raw_path.is_absolute() {
        raw_path = cwd.clone().join(raw_path);
    }

    let abs_path = match canonicalize(raw_path) {
        Ok(p) => p,
        Err(_) => {
            kprintln!("\ninvalid arg: {}", dir_arg);
            return None;
        }
    };

    Some(abs_path)
}

fn mkdir(cwd: &PathBuf, args: &[&str]) {
    let abs_path = match get_abs_path(cwd, args[0]) {
        Some(p) => p,
        None => return
    };

    let dir_metadata = fat32::vfat::Metadata {
        name: String::from(abs_path.file_name().unwrap().to_str().unwrap()),
        created: fat32::vfat::Timestamp::default(),
        accessed: fat32::vfat::Timestamp::default(),
        modified: fat32::vfat::Timestamp::default(),
        attributes: fat32::vfat::Attributes::default_dir(), // directory 
        size: 0
    };

    let path_clone = abs_path.clone();
    FILESYSTEM.create_dir(abs_path.parent().unwrap(), dir_metadata).expect("Failed to create dir");
    FILESYSTEM.flush_fs(path_clone);
}

fn write_file_test(cwd: &PathBuf) {
    use shim::io::Write;

    let mut dir = FILESYSTEM.open_dir(cwd.as_path()).expect("Couldn't get $CWD as dir");
    dir.create(fat32::vfat::Metadata {
        name: String::from("test_write.txt"),
        created: fat32::vfat::Timestamp::default(),
        accessed: fat32::vfat::Timestamp::default(),
        modified: fat32::vfat::Timestamp::default(),
        attributes: fat32::vfat::Attributes::default(),
        size: 0,
    }).expect("Couldn't create test_write.txt");
    let mut path = cwd.clone();
    path.push("test_write.txt");

    let test_file_entry = FILESYSTEM.open(path.as_path()).expect("couldn't open /test_write.txt");
    assert!(test_file_entry.is_file());
    let mut test_file = test_file_entry.into_file().expect("couldn't open /test_write.txt as file");
    let test_buf = "hello world!!\n".as_bytes();
    assert_eq!(test_file.write(test_buf).unwrap(), test_buf.len());
    assert_eq!(test_file.write(test_buf).unwrap(), test_buf.len());
    FILESYSTEM.flush_fs(cwd);
}

fn touch(cwd: &PathBuf, args: &[&str]) {
    for arg in args {
        let arg_path = PathBuf::from(arg);
        let raw_path = if !arg_path.is_absolute() {
            cwd.join(arg_path)
        } else { arg_path };
        let path = canonicalize(raw_path).expect("Could not canonicalize path");
        let base = path.parent();
        let mut base_dir = match base {
            None => FILESYSTEM.open_dir("/").expect("Could not get / as dir"),
            Some(base) => FILESYSTEM.open_dir(base).expect("Could not get target as dir"),
        };
        let file = path.file_name().expect("Must specify a file to create")
                       .to_str().expect("Couldn't get filename as string");
        base_dir.create(fat32::vfat::Metadata {
            name: String::from(file),
            ..Default::default()
        }).expect("Couldn't create file");
        match base {
            Some(base) => FILESYSTEM.flush_fs(base),
            None => FILESYSTEM.flush_fs("/")
        }
    }
}

fn append(cwd: &PathBuf, args: &[&str]) {
    use shim::io::{Write, Seek, SeekFrom};

    if args.len() < 2 {
        kprintln!("USAGE: append [filename] [contents]");
        return;
    }

    let arg_path = PathBuf::from(args[0]);
    let raw_path = if !arg_path.is_absolute() {
        cwd.join(arg_path)
    } else { arg_path };
    let path = canonicalize(raw_path).expect("Could not canonicalize path");
    let mut fd = FILESYSTEM.open_file(path.as_path()).expect("Couldn't open file for writing");

    for i in 1..args.len() {
        fd.seek(SeekFrom::End(0)).expect("Failed to seek to end of file");
        fd.write(&args[i].bytes().collect::<alloc::vec::Vec<u8>>()).expect("Failed to append to file");
        if i < args.len() - 1 {
            fd.write(&[' ' as u8]).expect("Failed to append space to file");
        }
    }
    fd.write(&['\n' as u8]).expect("Failed to append newline to file");

    FILESYSTEM.flush_fs(path);
}

fn rm(cwd: &PathBuf, args: &[&str]) {
    use fat32::traits::File;

    if args.len() < 1 {
        kprintln!("USAGE: rm [filename]+");
        return;
    }

    for i in 0..args.len() {
        let arg_path = PathBuf::from(args[i]);
        let raw_path = if !arg_path.is_absolute() {
            cwd.join(arg_path)
        } else { arg_path };
        let path = canonicalize(raw_path).expect("Could not canonicalize path");
        let fd = FILESYSTEM.open(path.as_path()).expect("Couldn't open file for writing");

        if fd.is_dir() {
            match fd.into_dir().expect("Couldn't get dir as dir").delete() {
                Ok(_) => (),
                Err(e) => kprintln!("Could not delete directory: {:?}", e),
            }
        } else {
            fd.into_file().expect("Couldn't get file as file").delete().expect("Could not delete file");
        }
        FILESYSTEM.flush_fs(path);
    }
}

fn mount(cwd: &PathBuf, args: &[&str]) {
    if args.len() < 2 {
        kprintln!("not enough arguments!\nusage: mount <part> <path> -p <pw>");
        return;
    }
    let part_num: usize = match args[0].parse() {
        Ok(num) => num,
        Err(_) => {
            kprintln!("invalid partition number");
            return;
        }
    };

    let abs_path = match get_abs_path(cwd, args[1]) {
        Some(p) => p,
        None => return
    };

    let mut mount_opts = MountOptions::Normal;
    if args.len() > 2 && args.len() != 4 {
        kprintln!("incorrect arguments!\nusage: mount <part> <path> -p <pw>");
        return;
    } else if args.len() > 2 {
        if args[2].eq_ignore_ascii_case("-p") {
            mount_opts = MountOptions::Encrypted(Some(String::from(args[3])));
        } else {
            kprintln!("unknown flag: {}", args[2]);
            return;
        }
    }


    FILESYSTEM.mount(part_num, abs_path, mount_opts);
}

fn umount(cwd: &PathBuf, mount_point: &str) {
    let abs_path = match get_abs_path(cwd, mount_point) {
        Some(p) => p,
        None => return
    };
    
    if abs_path.to_str().unwrap().eq_ignore_ascii_case("/") {
        kprintln!("don't unmount root!!!");
    }  else {
        match FILESYSTEM.unmount(PathBuf::from(abs_path.to_str().unwrap())) {
            Ok(_) => { kprintln!("unmounted {}", abs_path.to_str().unwrap()); },
            Err(_) => ()
        }
    }
}

// backs the mkcrypt command
// encrypts a device sector by sector
// usage: mkcrypt {header|full} num password
// header = don't encrypt data area (saves time if we don't care about the existing data on the partition)
// header mode will also zero the root cluster so that it's valid
// full = encrypt every sector of the partition (will be slow for large disks)
// if there's a power disruption or anything like that during the execution of this command you'll have
// big problems (we can fix that by creating a backup though)
fn encrypt_part(args: &[&str]) {
    if args.len() < 3 {
        kprintln!("not enough arguments!\nusage: mkcrypt {{header|full}} num password");
        return;
    }
    // check usage 
    let mode = args[0]; // args[0] = mode (header|full)
    match mode {
        "header"|"full" => (),
        _ => {
            kprintln!("invalid mode: {}!\nusage: mkcrypt {{header|full}} num password", mode);
            return;
        }
    }

    let part_num: usize = match args[1].parse() {
        Ok(num) => num,
        Err(_) => {
            kprintln!("invalid partition number: {}!\nusage: mkcrypt {{header|full}} num password", args[1]);
            return;
        }
    };

    if args[2].as_bytes().len() > 16 {
        kprintln!("password can be at most 16 bytes");
        return;
    }

    // we can change this later to be device agnostic
    let mbr = match MasterBootRecord::from(Sd {}) {
        Ok(mbr) => mbr,
        Err(e) => {
            kprintln!("error parsing MBR: {:?}", e);
            return;
        }
    };

    let start_sector = match mbr.get_partition_start(part_num) {
        Some(s) => s as u64,
        None => {
            kprintln!("unable to get start sector for partition #{}", part_num);
            return;
        }
    };

    let ebpb = match BiosParameterBlock::from(Sd {}, start_sector) {
        Ok(ebpb) => ebpb,
        Err(e) => {
            kprintln!("unable to parse EBPB: {:?}", e);
            return;
        }
    };

    let end_sector: u64;
    match mode {
        "header" => {
            end_sector = start_sector
                       + ebpb.num_reserved_sectors as u64
                       + (ebpb.num_fats as u64 * ebpb.sectors_per_fat as u64)
                       - 1;
            kprintln!("Encrypting root cluster...");
            // encrypt root dir as well
            let sector_offset = ((ebpb.root_cluster_number - 2) as u64) * (ebpb.sectors_per_cluster as u64);
            let root_start_sector = end_sector + 1 + sector_offset as u64;
            let root_cluster_end = root_start_sector + ebpb.sectors_per_cluster as u64 - 1;
            match encrypt_sectors(root_start_sector, root_cluster_end, args[2]) {
                Ok(_) => (),
                Err(_) => return
            }
            kprintln!("Encrypting filesystem header...");
        },
        "full" => {
            end_sector = start_sector
                       + ebpb.num_logical_sectors_ext as u64
                       - 1;
            kprintln!("Encrypting entire partition...");
        }
        _ => {
            kprintln!("invalid mode: {}!\nusage: mkcrypt {{header|full}} num password", mode);
            return;
        }
    }

    match encrypt_sectors(start_sector, end_sector, args[2]) {
            Ok(_) => (),
            Err(_) => return
    }
}

// encrypts sectors [first, last] on Sd {} using key
fn encrypt_sectors(first: u64, last: u64, key: &str) -> Result<(), ()> {
    kprintln!("about to encrypt sectors {} - {}, inclusive.", first, last);
    let mut plaintext_device = Sd {};
    // key should have been guaranteed valid by encrypt_part
    let mut crypt_device = EncryptedDevice::new(key, Sd {}).unwrap();
    let mut buf = [0u8; 512];

    let total_sectors: f64 = last as f64 - first as f64 + 1f64;
    for sector in first..=last {
        let progress = ((sector - first) as f64)/total_sectors * 100f64;
        kprint!("Encrypting sectors ({:.2}%)\r", progress);
        match plaintext_device.read_sector(sector, &mut buf) {
            Ok(_) => (),
            Err(_) => {
                kprintln!("error reading sector: {}. Exting.", sector);
                return Err(());
            }
        }
        match crypt_device.write_sector(sector, &buf) {
            Ok(_) => (),
            Err(_) => {
                kprintln!("error writing encrypted sector: {}. Exting.", sector);
                return Err(());
            }
        }
    }
    kprintln!("");
    Ok(())
}

/// Starts a shell using `prefix` as the prefix for each line. This function
/// never returns.
pub fn shell(prefix: &str) {
    use core::str;

    let mut path_buf = PathBuf::from("/");

    loop {
        let mut text_idx = 0;
        let mut text_buf = [0u8; 512];
        let mut args_buf = [""; 64];
        kprint!("[{}]{} ", path_buf.to_str().unwrap_or_default(), prefix);
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
            Ok(cmd) =>
                if cmd.args[0] == "exit" {
                    return
                } else {
                    cmd.eval(&mut path_buf)
                }
        }
    }
}
