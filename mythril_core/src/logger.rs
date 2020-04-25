use core::fmt;
use core::fmt::Write;
use spin::Mutex;

static LOG_LOCK: Mutex<()> = Mutex::new(());

pub fn write_console(s: impl AsRef<str>) {
    let lock = LOG_LOCK.lock();
    unsafe {
        raw_write_console(s)
    };
    drop(lock)
}

// NOTE: the caller should hold `LOG_LOCK`
pub unsafe fn raw_write_console(s: impl AsRef<str>) {
    //FIXME: what about addresses above 4GB?
    let len = s.as_ref().len();
    let ptr = s.as_ref().as_ptr();

    asm!("cld; rep outsb"
         :
         :"{rdx}"(0x3f8), "{rcx}"(len as u64), "{rsi}"(ptr as u64)
         : "rflags", "rsi"
         : "volatile");
}

pub struct DirectLogger;
impl DirectLogger {
    pub const fn new() -> Self {
        DirectLogger {}
    }
}

impl log::Log for DirectLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
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
        unsafe { raw_write_console(s) };
        Ok(())
    }

    fn write_fmt(&mut self, args: fmt::Arguments) -> Result<(), fmt::Error> {
        // Acquire the lock at the formatting stage, so the args will not
        // race with the guest console (that calls `write_console` directly)
        let lock = LOG_LOCK.lock();
        let ret = fmt::write(self, args);
        drop(lock);
        return ret;
    }
}
