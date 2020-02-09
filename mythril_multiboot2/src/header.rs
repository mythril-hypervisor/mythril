
#[repr(packed(1))]
pub struct Multiboot2Tag {
    typ: u16,
    flags: u16,
    size: u32
}

#[repr(packed(1))]
pub struct Multiboot2Header {
    magic: u32,
    arch: u32,
    header_len: u32,
    checksum: u32,
    end_tag: Multiboot2Tag
}

#[no_mangle]
pub static MULTIBOOT_HEADER: Multiboot2Header = Multiboot2Header {
    magic: 0xe85250d6,
    arch: 0,
    header_len: core::mem::size_of::<Multiboot2Header>() as u32,
    checksum: (0x100000000 - (0xe85250d6 + 0 + core::mem::size_of::<Multiboot2Header>()) as u64) as u32,
    end_tag: Multiboot2Tag {
        typ: 0,
        flags: 0,
        size: 8
    }
};
