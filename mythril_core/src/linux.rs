use crate::device::qemu_fw_cfg::{FwCfgSelector, QemuFwCfgBuilder};
use crate::error::{Error, Result};
use crate::vm::VmServices;
use bitflags::bitflags;
use byteorder::{ByteOrder, LittleEndian};

bitflags! {
    pub struct XLoadFlags: u32 {
        const KERNEL_64 = 1 << 0;
        const CAN_BE_LOADED_ABOVE_4G = 1 << 1;
        const EFI_HANDOVER_32 = 1 << 2;
        const EFI_HANDOVER_64 = 1 << 3;
        const EFI_KEXEC = 1 << 4;
        const FIVE_LEVEL = 1 << 5;
        const FIVE_LEVEL_ENABLED = 1 << 6;
    }
}

pub fn load_linux(
    kernel_name: impl AsRef<str>,
    initramfs_name: impl AsRef<str>,
    cmdline: &[u8],
    memory: u64,
    builder: &mut QemuFwCfgBuilder,
    services: &mut impl VmServices,
) -> Result<()> {
    let mut kernel = services.read_file(kernel_name.as_ref())?.to_vec();
    let initramfs = services.read_file(initramfs_name.as_ref())?;

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

    info!("Protocol = 0x{:x}", protocol);

    let mut initrd_max = if protocol >= 0x20c
        && (LittleEndian::read_u32(&kernel[0x236..0x236 + 4])
            & XLoadFlags::CAN_BE_LOADED_ABOVE_4G.bits())
            != 0
    {
        0xffffffff
    } else if protocol >= 0x203 {
        LittleEndian::read_u32(&kernel[0x22c..0x22c + 4])
    } else {
        0x37ffffff
    };

    // Don't position the initramfs above the available memory
    if (memory < 4 * 1024) && (initrd_max as u64 >= (memory << 20)) {
        initrd_max = (memory << 20) as u32 - 1;
    }

    builder.add_i32(FwCfgSelector::CMDLINE_ADDR, cmdline_addr);
    builder.add_bytes(FwCfgSelector::CMDLINE_DATA, cmdline);
    //TODO: this should be NULL terminated
    builder.add_i32(FwCfgSelector::CMDLINE_SIZE, cmdline.len() as i32);

    if protocol >= 0x202 {
        LittleEndian::write_i32(&mut kernel[0x228..0x228 + 4], cmdline_addr);
    } else {
        LittleEndian::write_u16(&mut kernel[0x20..0x20 + 2], 0xa33f);
        LittleEndian::write_i16(
            &mut kernel[0x22..0x22 + 2],
            (cmdline_addr - real_addr) as i16,
        );
    }

    //TODO: vga parameters

    // loader type
    // TODO: change this from QEMU probably
    if protocol >= 0x200 {
        kernel[0x210] = 0xB0;
    }

    // Heap
    if protocol >= 0x201 {
        kernel[0x211] |= 0x80;
        LittleEndian::write_i16(
            &mut kernel[0x224..0x224 + 2],
            (cmdline_addr - real_addr - 0x200) as i16,
        );
    }

    if protocol < 0x200 {
        return Err(Error::InvalidValue(
            "Kernel too old for initrd support".into(),
        ));
    }

    if initramfs.len() as u32 > initrd_max {
        return Err(Error::InvalidValue(format!(
            "Initramfs too large (0x{:x} bytes > max of 0x{:x})",
            initramfs.len(),
            initrd_max
        )));
    }

    let initrd_addr = ((initrd_max - initramfs.len() as u32) & !4095) as i32;
    builder.add_i32(FwCfgSelector::INITRD_ADDR, initrd_addr);
    builder.add_i32(FwCfgSelector::INITRD_SIZE, initramfs.len() as i32);
    builder.add_bytes(FwCfgSelector::INITRD_DATA, initramfs);
    LittleEndian::write_i32(&mut kernel[0x218..0x218 + 4], initrd_addr);
    LittleEndian::write_i32(&mut kernel[0x21c..0x21c + 4], initramfs.len() as i32);

    let setup_size = match kernel[0x1f1] {
        // For legacy compat, setup size 0 is really 4 sectors
        0 => 4 + 1,
        size => size + 1,
    } as i32
        * 512;

    if setup_size as usize > kernel.len() {
        return Err(Error::InvalidValue(
            "Invalid kernel header (setup size > header size)".into(),
        ));
    }
    let kernel_size = kernel.len() as i32 - setup_size;

    builder.add_i32(FwCfgSelector::KERNEL_ADDR, prot_addr);
    builder.add_i32(FwCfgSelector::KERNEL_SIZE, kernel_size);
    builder.add_bytes(FwCfgSelector::KERNEL_DATA, &kernel[setup_size as usize..]);

    builder.add_i32(FwCfgSelector::SETUP_ADDR, real_addr);
    builder.add_i32(FwCfgSelector::SETUP_SIZE, setup_size);
    //TODO: this should never be _more_ than 8k
    builder.add_bytes(FwCfgSelector::SETUP_DATA, &kernel[..setup_size as usize]);

    info!("CMDLINE_ADDR: 0x{:x}", cmdline_addr);
    info!("CMDLINE_SIZE: 0x{:x}", cmdline.len());
    info!("KERNEL_ADDR: 0x{:x}", prot_addr);
    info!("KERNEL_SIZE: 0x{:x}", kernel_size);
    info!("SETUP_ADDR: 0x{:x}", real_addr);
    info!("SETUP_SIZE: 0x{:x}", setup_size);
    info!("INITRD_ADDR: 0x{:x}", initrd_addr);
    info!("INITRD_SIZE: 0x{:x}", initramfs.len());
    Ok(())
}
