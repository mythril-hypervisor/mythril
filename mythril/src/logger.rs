use crate::time;

use core::fmt;
use core::fmt::Write;
use spin::Mutex;

static LOG_LOCK: Mutex<()> = Mutex::new(());
static mut VGA_WRITER: VgaWriter = VgaWriter::new();

const VGA_BASE_ADDR: usize = 0xB8000;
const VGA_WIDTH: usize = 80;
const VGA_HEIGHT: usize = 25;
const VGA_ATTRIB: u16 = 0x0F00; // black background, white text

fn scroll_vga(vga_mem: &mut [[u16; VGA_WIDTH]; VGA_HEIGHT]) {
    for row in 1..VGA_HEIGHT {
        for col in 0..VGA_WIDTH {
            vga_mem[row - 1][col] = vga_mem[row][col];
        }
    }
    clear_line_vga(VGA_HEIGHT - 1, vga_mem);
}

fn clear_line_vga(row: usize, vga_mem: &mut [[u16; VGA_WIDTH]; VGA_HEIGHT]) {
    for col in 0..VGA_WIDTH {
        (*vga_mem)[row][col] = VGA_ATTRIB | 0x20;
    }
}

pub fn clear_vga(vga_mem: &mut [[u16; VGA_WIDTH]; VGA_HEIGHT]) {
    for row in 0..VGA_HEIGHT {
        clear_line_vga(row, vga_mem);
    }
}

pub fn raw_write_vga(
    s: impl AsRef<str>,
    mut col: usize,
    mut row: usize,
    vga_mem: &mut [[u16; VGA_WIDTH]; VGA_HEIGHT],
) -> (usize, usize) {
    for byte in s.as_ref().bytes() {
        // move cursor on newlines (0x0A) and carriage-returns (0x0D)
        if byte == 0x0A {
            row += 1;
            col = 0;
            continue;
        } else if byte == 0x0D {
            col = 0;
            continue;
        }

        if row >= VGA_HEIGHT {
            scroll_vga(vga_mem);
            row = VGA_HEIGHT - 1;
        }

        vga_mem[row][col] = VGA_ATTRIB | (byte as u16);

        col += 1;

        if col >= VGA_WIDTH {
            row += 1;
            col = 0;
        }
    }

    (col, row)
}

pub fn write_console(s: impl AsRef<str>) {
    let lock = LOG_LOCK.lock();
    unsafe { raw_write_console(s) };
    drop(lock)
}

// NOTE: the caller should hold `LOG_LOCK`
pub unsafe fn raw_write_console(s: impl AsRef<str>) {
    // mirror console output to VGA
    VGA_WRITER.write(s.as_ref());

    //FIXME: what about addresses above 4GB?
    let len = s.as_ref().len();
    let ptr = s.as_ref().as_ptr();

    asm!(
        "cld",
        "rep outsb",
        in("rdx") 0x3f8,
        in("rcx") len as u64,
        inout("rsi") ptr as u64 => _,
        options(nostack)
    );
}

pub struct VgaWriter {
    cur_col: usize,
    cur_row: usize,
}

impl VgaWriter {
    pub const fn new() -> Self {
        VgaWriter {
            cur_col: 0,
            cur_row: 0,
        }
    }

    pub fn write(&mut self, s: impl AsRef<str>) {
        let mut vga_mem = unsafe { &mut *(VGA_BASE_ADDR as *mut _) };
        if self.cur_col == 0 && self.cur_row == 0 {
            clear_vga(&mut vga_mem);
        }
        let (col, row) =
            raw_write_vga(s, self.cur_col, self.cur_row, &mut vga_mem);
        self.cur_col = col;
        self.cur_row = row;
    }
}

impl fmt::Write for VgaWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write(s);
        Ok(())
    }

    fn write_fmt(&mut self, args: fmt::Arguments) -> Result<(), fmt::Error> {
        fmt::write(self, args)
    }
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
        let (stamp_sec, stamp_subsec) = if time::is_global_time_ready() {
            let diff = time::now() - time::system_start_time();
            (diff.as_secs(), diff.subsec_micros())
        } else {
            (0, 0)
        };
        writeln!(
            DirectWriter {},
            "[{:>4}.{:06}] MYTHRIL-{}: {}",
            stamp_sec,
            stamp_subsec,
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
