#[derive(Debug)]
pub enum Error {
    VmWriteError,
    VmReadError,
    AllocError(&'static str)
}

pub type Result<T> = core::result::Result<T, Error>;
