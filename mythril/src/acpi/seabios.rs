//! Structures and methods for using the seabios table-loader feature
//!
//! # Basics
//!
//! If seabios is compiled with the `CONFIG_FW_ROMFILE_LOAD` option,
//! seabios has a feature that will allow the user to write the
//! ACPI tables by loading a fw_cfg file to `etc/table-loader`.
//! The table loader may contain a series of commands that allocate
//! bytes and copy them to a bios data region, add pointers to a
//! previously allocated region, update the checksum value, or write
//! a pointer back to a fw_cfg file.
//!
//! # Commands
//!
//! ## Allocate
//!
//! Allocates the contents of a fw_cfg file in a bios data region.
//! This command must occur first for any fw_cfg file that will be
//! used by the add checksum or add pointer command.
//!
//! ## Add Pointer
//!
//! Adds a pointer of the given size at the given offset in a
//! destination table with points to the given source table. In
//! practice this is helpful for building the RSDT or XSDT with
//! an add pointer command occuring for each SDT in the root table.
//!
//! ## Write Pointer
//!
//! Writes a pointer back to a destination host file.
//!
//! ## Checksum
//!
//! Updates the checksum of a given table at a given offset. The checksum
//! is calculated for the range given and added to the current checksum
//! in the table.
//!
//! # Examples
//!
//! ```
//! # use mythril::acpi::seabios::{AllocZone, TableLoaderBuilder, TableLoaderCommand};
//! // Create a backing buffer and the table loader builder
//! let tl_buf = [0x00; 512];
//! let mut tl_builder = TableLoaderBuilder::new(&mut tl_buf[..]).unwrap();
//!
//! // Allocate a previously loaded fw_cfg file
//! tl_builder.add_command(TableLoaderCommand::Allocate {
//!     file: "etc/mythril/myrsdp",
//!     align: 0x10,
//!     zone: AllocZone::Fseg,
//! }).unwrap();
//!
//! // Allocate a previously created RSDT
//! tl_builder.add_command(TableLoaderCommand::Allocate {
//!     file: "etc/mythril/myrsdt",
//!     align: 0x10,
//!     zone: AllocZone::Fseg,
//! }).unwrap();
//!
//! // Add a pointer to the created RSDT to the RSDP
//! tl_builder.add_command(TableLoaderCommand::AddPointer {
//!     src: "etc/mythril/myrsdt",
//!     dst: "etc/mythril/myrsdp",
//!     offset: 16,
//!     size: 4,
//! }).unwrap();
//! ```
//!
//! # Protocol
//!
//! ## Command Structure
//!
//! All entries must be 128 bytes long and the first 4 bytes contain
//! the command type. The command types currently supported are as
//! follows:
//!
//!  - 1: Allocate
//!  - 2: Add Pointer
//!  - 3: Add Checksum
//!  - 4: Write Pointer
//!
//! All numbers that are more than one byte are Little Endian.

use crate::error::{Error, Result};
use crate::virtdev::qemu_fw_cfg::QemuFwCfgBuilder;

use arrayvec::{Array, ArrayVec};
use byteorder::{ByteOrder, LittleEndian};
use num_enum::TryFromPrimitive;

/// The maximum size for a given romfile.
const ROMFILE_FILENAME_SIZE: usize = 56;

/// The allocation zone for the given romfile.
#[repr(u8)]
#[derive(Copy, Clone, Debug, TryFromPrimitive)]
pub enum AllocZone {
    /// The high allocation zone
    High = 0x01,
    /// The fseg allocation zone
    Fseg = 0x02,
}

/// A command entry for the table loader.
pub enum TableLoaderCommand<'a> {
    /// Allocate a table from `file`.
    ///
    /// Must appear exactly once for each file, and before
    /// this file is referenced by any other command.
    Allocate {
        /// The file to allocate
        file: &'a str,
        /// The alignment the alloc is subject to
        align: u32,
        /// May be FSEG or HIGH.
        zone: AllocZone,
    },
    /// Patch the given table with a pointer to another table
    AddPointer {
        /// The table to patch
        dst: &'a str,
        /// The file the pointer added will point to
        src: &'a str,
        /// The offset in `dst` the pointer should be added at
        offset: u32,
        /// The size of the pointer to be added
        size: u8,
    },
    /// Update the checksum of the given table
    AddChecksum {
        /// The table the checksum needs to be updated in
        file: &'a str,
        /// The offset of the checksum
        offset: u32,
        /// The start offset the checksum calculation should star at
        start: u32,
        /// The length of the checksum buffer to calculate
        length: u32,
    },
    /// Write back to a host file
    WritePointer {
        /// The table to patch
        dst: &'a str,
        /// The file the pointer added will point to
        src: &'a str,
        /// The offset in `dst` the pointer should be added at
        dst_offset: u32,
        /// The offset in `src` the pointer should point to
        src_offset: u32,
        /// THe size of the pointer
        size: u8,
    },
}

impl<'a> TableLoaderCommand<'a> {
    fn encode<T: Array<Item = u8>>(
        &self,
        buffer: &mut ArrayVec<T>,
    ) -> Result<()> {
        let mut bytes = [0x00; 128];
        match self {
            Self::Allocate { file, align, zone } => {
                if file.len() >= ROMFILE_FILENAME_SIZE {
                    Err(Error::NotSupported)
                } else {
                    // Write out the CMD_ALLOC command
                    LittleEndian::write_u32(&mut bytes[..4], 1);
                    bytes[4..(4 + file.len())].copy_from_slice(file.as_bytes());
                    LittleEndian::write_u32(&mut bytes[60..64], *align);
                    bytes[64] = *zone as u8;
                    Ok(())
                }
            }
            Self::AddPointer {
                dst,
                src,
                offset,
                size,
            } => {
                if src.len() >= ROMFILE_FILENAME_SIZE
                    || dst.len() >= ROMFILE_FILENAME_SIZE
                {
                    Err(Error::NotSupported)
                } else {
                    LittleEndian::write_u32(&mut bytes[..4], 2);
                    bytes[4..(4 + dst.len())].copy_from_slice(dst.as_bytes());
                    bytes[60..(60 + src.len())].copy_from_slice(src.as_bytes());
                    LittleEndian::write_u32(&mut bytes[116..120], *offset);
                    bytes[120] = *size;
                    Ok(())
                }
            }
            Self::AddChecksum {
                file,
                offset,
                start,
                length,
            } => {
                if file.len() >= ROMFILE_FILENAME_SIZE {
                    Err(Error::NotSupported)
                } else {
                    LittleEndian::write_u32(&mut bytes[..4], 3);
                    bytes[4..(4 + file.len())].copy_from_slice(file.as_bytes());
                    LittleEndian::write_u32(&mut bytes[60..64], *offset);
                    LittleEndian::write_u32(&mut bytes[64..68], *start);
                    LittleEndian::write_u32(&mut bytes[68..72], *length);
                    Ok(())
                }
            }
            Self::WritePointer {
                dst,
                src,
                dst_offset,
                src_offset,
                size,
            } => {
                if src.len() >= ROMFILE_FILENAME_SIZE
                    || dst.len() >= ROMFILE_FILENAME_SIZE
                {
                    Err(Error::NotSupported)
                } else {
                    LittleEndian::write_u32(&mut bytes[..4], 4);
                    bytes[4..(4 + dst.len())].copy_from_slice(dst.as_bytes());
                    bytes[60..(60 + src.len())].copy_from_slice(src.as_bytes());
                    LittleEndian::write_u32(&mut bytes[116..120], *dst_offset);
                    LittleEndian::write_u32(&mut bytes[120..124], *src_offset);
                    bytes[124] = *size;
                    Ok(())
                }
            }
        }?;
        buffer.try_extend_from_slice(&bytes)?;
        Ok(())
    }
}

/// Builder structure for a seabios table loader
pub struct TableLoaderBuilder<T: Array<Item = u8>>(ArrayVec<T>);

impl<T: Array<Item = u8>> TableLoaderBuilder<T> {
    const TABLE_LOADER_NAME: &'static str = "etc/table-loader";

    /// Create a empty table loader
    pub fn new() -> Result<Self> {
        Ok(TableLoaderBuilder(ArrayVec::<T>::new()))
    }

    /// Add the given command entry to the table loader
    pub fn add_command(&mut self, cmd: TableLoaderCommand) -> Result<()> {
        cmd.encode(&mut self.0)
    }

    /// Load the table loader to a fw_cfg file
    pub fn load(
        &mut self,
        fw_cfg_builder: &mut QemuFwCfgBuilder,
    ) -> Result<()> {
        fw_cfg_builder.add_file(Self::TABLE_LOADER_NAME, self.0.as_slice())?;
        Ok(())
    }
}
