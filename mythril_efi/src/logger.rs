use core::fmt::{self, Write};
use core::convert::AsRef;

fn write_console(s: impl AsRef<str>) {
    //FIXME: what about addresses above 4GB?
    //FIXME: should we lock to prevent partial strings on the console?
    let bytes = s.as_ref().as_bytes();
    let len = bytes.len();
    let ptr = bytes.as_ptr();
    unsafe {
        asm!("rep outsb"
             :
             :"{dx}"(0x3f8), "{ecx}"(len as u32), "{esi}"(ptr as u32));
    }
}

// FIXME: We should probably keep a buffer in the logger struct to avoid
//        needing to allocate a bunch here.
pub struct EfiLogger;
impl log::Log for EfiLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        write_console(format!("{}: {}\n",
                              record.level(),
                              record.args()));
    }

    fn flush(&self) {
        // This simple logger does not buffer output.
    }
}
