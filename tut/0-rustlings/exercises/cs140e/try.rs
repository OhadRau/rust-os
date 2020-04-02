use std::try;

struct ErrorA;
struct ErrorB;

enum Error {
    A(ErrorA),
    B(ErrorB)
}

// What traits does `Error` need to implement?
impl std::convert::From<ErrorA> for Error {
    fn from(e: ErrorA) -> Error {
        Error::A(e)
    }
}

impl std::convert::From<ErrorB> for Error {
    fn from(e: ErrorB) -> Error {
        Error::B(e)
    }
}

fn do_a() -> Result<u16, ErrorA> {
    Err(ErrorA)
}

fn do_b() -> Result<u32, ErrorB> {
    Err(ErrorB)
}

fn do_both() -> Result<(u16, u32), Error> {
    let a = try!(do_a());
    let b = try!(do_b());
    Ok((a, b))
}

fn main() { }
