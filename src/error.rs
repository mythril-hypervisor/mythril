#[derive(Debug)]
pub enum Error {
    VmFailInvalid,
    VmFailValid,
    AllocError(&'static str),
}

pub type Result<T> = core::result::Result<T, Error>;
