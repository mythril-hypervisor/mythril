use crate::boot_info::BootInfo;
use crate::error::{Error, Result};
use crate::virtdev::qemu_fw_cfg::{FwCfgSelector, QemuFwCfgBuilder};
use bitflags::bitflags;
use byteorder::{ByteOrder, LittleEndian};

// These come mostly from https://www.kernel.org/doc/Documentation/x86/boot.txt
mod offsets {
    use core::ops::Range;
    pub const OLD_CMD_LINE_MAGIC: Range<usize> = 0x20..0x22;
    pub const SETUP_SECTS: usize = 0x1f1;
    pub const OLD_CMD_LINE_OFFSET: Range<usize> = 0x22..0x24;
    pub const HEADER_MAGIC: Range<usize> = 0x202..0x206;
    pub const BOOTP_VERSION: Range<usize> = 0x206..0x208;
    pub const TYPE_OF_LOADER: usize = 0x210;
    pub const LOAD_FLAGS: usize = 0x211;
    pub const RAMDISK_IMAGE: Range<usize> = 0x218..0x21c;
    pub const RAMDISK_SIZE: Range<usize> = 0x21c..0x220;
    pub const HEAP_END_PTR: Range<usize> = 0x224..0x226;
    pub const CMD_LINE_PTR: Range<usize> = 0x228..0x22c;
    pub const INITRD_ADDR_MAX: Range<usize> = 0x22c..0x230;
    pub const XLOAD_FLAGS: Range<usize> = 0x236..0x238;
}

const HEADER_MAGIC_VALUE: u32 = 0x53726448; // "HdrS"
const OLD_CMD_LINE_MAGIC_VALUE: u16 = 0xa33f;
const QEMU_LOADER: u8 = 0xb0;

// This blob is taken from QEMU. See:
// https://github.com/qemu/qemu/blob/887adde81d1f1f3897f1688d37ec6851b4fdad86/pc-bios/optionrom/linuxboot_dma.c
pub const LINUXBOOT_DMA_ROM: &'static [u8] =
    include_bytes!("blob/linuxboot_dma.bin");

bitflags! {
    pub struct LoadFlags: u8 {
        const LOADED_HIGH = 1 << 0;
        const KASLR = 1 << 1;
        const QUIET = 1 << 5;
        const KEEP_SEGMENTS = 1 << 6;
        const CAN_USE_HEAP = 1 << 7;
    }
}

bitflags! {
    pub struct XLoadFlags: u16 {
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
    info: &BootInfo,
) -> Result<()> {
    let mut kernel = info
        .find_module(kernel_name.as_ref())
        .ok_or_else(|| {
            Error::InvalidValue(format!(
                "No such kernel '{}'",
                kernel_name.as_ref()
            ))
        })?
        .data()
        .to_vec();
    let initramfs = info
        .find_module(initramfs_name.as_ref())
        .ok_or_else(|| {
            Error::InvalidValue(format!(
                "No such initramfs '{}'",
                initramfs_name.as_ref()
            ))
        })?
        .data();

    if kernel.len() < 8192 {
        return Err(Error::InvalidValue(format!(
            "Kernel image is too small ({} < 8192)",
            kernel.len()
        )));
    }

    let magic = LittleEndian::read_u32(&kernel[offsets::HEADER_MAGIC]);

    // HdrS
    if magic != HEADER_MAGIC_VALUE {
        return Err(Error::InvalidValue(format!(
            "Invalid kernel image (bad magic = 0x{:x})",
            magic
        )));
    }

    let protocol = LittleEndian::read_u16(&kernel[offsets::BOOTP_VERSION]);
    let (real_addr, cmdline_addr, prot_addr) =
        if protocol < 0x200 || (kernel[offsets::LOAD_FLAGS] & LoadFlags::LOADED_HIGH.bits()) == 0 {
            (0x90000, 0x9a000 - cmdline.len() as i32, 0x10000)
        } else if protocol < 0x202 {
            (0x90000, 0x9a000 - cmdline.len() as i32, 0x100000)
        } else {
            (0x10000, 0x20000, 0x100000)
        };

    info!("Protocol = 0x{:x}", protocol);

    let mut initrd_max = if protocol >= 0x20c
        && (LittleEndian::read_u16(&kernel[offsets::XLOAD_FLAGS])
            & XLoadFlags::CAN_BE_LOADED_ABOVE_4G.bits())
            != 0
    {
        0xffffffff
    } else if protocol >= 0x203 {
        LittleEndian::read_u32(&kernel[offsets::INITRD_ADDR_MAX])
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
        LittleEndian::write_i32(&mut kernel[offsets::CMD_LINE_PTR], cmdline_addr);
    } else {
        LittleEndian::write_u16(&mut kernel[offsets::OLD_CMD_LINE_MAGIC],
                                OLD_CMD_LINE_MAGIC_VALUE);
        LittleEndian::write_i16(
            &mut kernel[offsets::OLD_CMD_LINE_OFFSET],
            (cmdline_addr - real_addr) as i16,
        );
    }

    //TODO: vga parameters

    // loader type
    // TODO: change this from QEMU probably
    if protocol >= 0x200 {
        kernel[offsets::TYPE_OF_LOADER] = QEMU_LOADER;
    }

    // Heap
    if protocol >= 0x201 {
        kernel[offsets::LOAD_FLAGS] |= LoadFlags::CAN_USE_HEAP.bits();
        LittleEndian::write_i16(
            &mut kernel[offsets::HEAP_END_PTR],
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
    LittleEndian::write_i32(&mut kernel[offsets::RAMDISK_IMAGE], initrd_addr);
    LittleEndian::write_i32(
        &mut kernel[offsets::RAMDISK_SIZE],
        initramfs.len() as i32,
    );

    let setup_size = match kernel[offsets::SETUP_SECTS] {
        // For legacy compat, setup size 0 is really 4 sectors
        0 => 4 + 1,
        size => size + 1,
    } as i32
        * 512;

    if setup_size as usize > kernel.len() {
        return Err(Error::InvalidValue(
            "Invalid kernel header (setup size > kernel size)".into(),
        ));
    }
    let kernel_size = kernel.len() as i32 - setup_size;

    builder.add_i32(FwCfgSelector::KERNEL_ADDR, prot_addr);
    builder.add_i32(FwCfgSelector::KERNEL_SIZE, kernel_size);
    builder
        .add_bytes(FwCfgSelector::KERNEL_DATA, &kernel[setup_size as usize..]);

    builder.add_i32(FwCfgSelector::SETUP_ADDR, real_addr);
    builder.add_i32(FwCfgSelector::SETUP_SIZE, setup_size);
    builder
        .add_bytes(FwCfgSelector::SETUP_DATA, &kernel[..setup_size as usize]);

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
