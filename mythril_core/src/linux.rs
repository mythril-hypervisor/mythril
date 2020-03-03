use crate::device::qemu_fw_cfg::{FwCfgSelector, QemuFwCfgBuilder};
use crate::error::{Error, Result};
use crate::vm::VmServices;
use byteorder::{ByteOrder, LittleEndian};

pub fn load_linux(
    builder: &mut QemuFwCfgBuilder,
    services: &mut impl VmServices,
    cmdline: &[u8],
) -> Result<()> {
    let kernel = services.read_file("kernel")?;
    let initramfs = services.read_file("initramfs")?;

    if kernel.len() < 8192 {
        return Err(Error::InvalidValue(format!(
            "Kernel image is too small ({} < 8192)",
            kernel.len()
        )));
    }

    let magic = LittleEndian::read_u32(&kernel[0x202..0x202 + 4]);

    // HdrS
    if magic != 0x53726448 {
        return Err(Error::InvalidValue(format!(
            "Invalid kernel image (bad magic = 0x{:x})",
            magic
        )));
    }

    let protocol = LittleEndian::read_u16(&kernel[0x206..0x206 + 2]);
    let (real_addr, cmdline_addr, prot_addr) = if protocol < 0x200 || (kernel[0x211] & 0x01) == 0 {
        (0x90000, 0x9a000 - cmdline.len() as i32, 0x10000)
    } else if protocol < 0x202 {
        (0x90000, 0x9a000 - cmdline.len() as i32, 0x100000)
    } else {
        (0x10000, 0x20000, 0x100000)
    };

    builder.add_i32(FwCfgSelector::CMDLINE_ADDR, cmdline_addr);
    builder.add_bytes(FwCfgSelector::CMDLINE_DATA, cmdline);
    //TODO: this should be NULL terminated
    builder.add_i32(FwCfgSelector::CMDLINE_SIZE, cmdline.len() as i32);

    Ok(())
}
