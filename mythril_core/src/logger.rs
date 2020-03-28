use core::fmt;
use core::fmt::Write;
use spin::Mutex;

pub fn write_console(s: impl AsRef<str>) {
    //FIXME: what about addresses above 4GB?
    let len = s.as_ref().len();
    let ptr = s.as_ref().as_ptr();
    unsafe {
        asm!("cld; rep outsb"
             :
             :"{dx}"(0x3f8), "{rcx}"(len as u32), "{rsi}"(ptr as u32)
             : "rflags");
    }
}

pub struct DirectLogger {
    lock: Mutex<()>
}
impl DirectLogger {
    pub const fn new() -> Self {
        DirectLogger {
            lock: Mutex::new(())
        }
    }
}

impl log::Log for DirectLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        let _lock = self.lock.lock();
        writeln!(
            DirectWriter {},
            "MYTHRIL-{}: {}",
            record.level(),
            *record.args()
        )
        .unwrap();
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
