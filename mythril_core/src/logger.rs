use core::fmt;
use core::fmt::Write;

pub fn write_console(s: &str) {
    //FIXME: what about addresses above 4GB?
    //FIXME: should we lock to prevent partial strings on the console?
    // let bytes = s.as_ref().as_bytes();
    let len = s.len();
    let ptr = s.as_ptr();
    unsafe {
        asm!("cld; rep outsb"
             :
             :"{dx}"(0x3f8), "{rcx}"(len as u32), "{rsi}"(ptr as u32)
             : "rflags");
    }
}

// FIXME: We should probably keep a buffer in the logger struct to avoid
//        needing to allocate a bunch here.
pub struct DirectLogger;
impl log::Log for DirectLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        writeln!(DirectWriter {}, "{}", *record.args()).unwrap();
    }

    fn flush(&self) {
        // This simple logger does not buffer output.
    }
}

struct DirectWriter;
impl fmt::Write for DirectWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        write_console(s);
        Ok(())
    }
}
