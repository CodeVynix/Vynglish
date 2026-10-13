//! A hand-written PE32+ writer for a 64-bit Windows executable.
//!
//! The image has two sections (.text with the code, .data with globals and the
//! import table), image base fixed at `IMAGE_BASE` so nothing needs relocating,
//! and one import from kernel32.dll so the loader binds the three functions the
//! compiled program calls. Every reference an imported function call uses is a
//! fixed absolute address, so no dynamic initialization is needed.

use crate::compiler::x64::{DATA_RVA, IMAGE_BASE, TEXT_RVA};

const SECTION_ALIGN: u32 = 0x1000;
const FILE_ALIGN: u32 = 0x200;
const HEADER_SIZE: u32 = 0x400;
const TEXT_RAW: u32 = 0x400;

fn align(value: u32, to: u32) -> u32 {
    value.div_ceil(to) * to
}

/// Assemble a loadable PE file. `import_rva` points at the import descriptor
/// inside `data`, placed there by the compiler.
pub fn write(text: &[u8], data: &[u8], import_rva: u32) -> Vec<u8> {
    let text_raw = align(text.len() as u32, FILE_ALIGN);
    let data_raw = align(data.len() as u32, FILE_ALIGN);

    let mut out = Vec::new();

    // DOS header: "MZ" plus the offset of the PE signature.
    let mut dos = vec![0u8; 0x40];
    dos[0..2].copy_from_slice(b"MZ");
    dos[0x3C..0x40].copy_from_slice(&0x40u32.to_le_bytes());
    out.extend_from_slice(&dos);

    // PE signature.
    out.extend_from_slice(b"PE\0\0");

    // COFF header.
    push_u16(&mut out, 0x8664); // machine: AMD64
    push_u16(&mut out, 2); // number of sections
    push_u32(&mut out, 0); // timestamp: never the current time, so builds differ
    push_u32(&mut out, 0); // pointer to symbol table
    push_u32(&mut out, 0); // number of symbols
    push_u16(&mut out, 0xF0); // size of optional header
    push_u16(&mut out, 0x0022); // executable | large address aware

    // PE32+ optional header.
    push_u16(&mut out, 0x20B); // magic
    push_u8(&mut out, 1); // major linker
    push_u8(&mut out, 0);
    push_u32(&mut out, text_raw); // size of code
    push_u32(&mut out, data_raw); // size of initialized data
    push_u32(&mut out, 0); // size of uninitialized data
    push_u32(&mut out, TEXT_RVA); // address of entry point
    push_u32(&mut out, TEXT_RVA); // base of code
    push_u64(&mut out, IMAGE_BASE);
    push_u32(&mut out, SECTION_ALIGN);
    push_u32(&mut out, FILE_ALIGN);
    push_u16(&mut out, 6); // major OS version
    push_u16(&mut out, 0);
    push_u16(&mut out, 6); // major image version
    push_u16(&mut out, 0);
    push_u16(&mut out, 6); // major subsystem version
    push_u16(&mut out, 0);
    push_u32(&mut out, 0); // win32 version value
    push_u32(&mut out, DATA_RVA + align(data.len() as u32, SECTION_ALIGN)); // size of image
    push_u32(&mut out, HEADER_SIZE); // size of headers
    push_u32(&mut out, 0); // checksum
    push_u16(&mut out, 3); // subsystem: windows console
    push_u16(&mut out, 0); // dll characteristics: no ASLR, so no relocations
    push_u64(&mut out, 0x10_0000); // stack reserve
    push_u64(&mut out, 0x1000); // stack commit
    push_u64(&mut out, 0x10_0000); // heap reserve
    push_u64(&mut out, 0x1000); // heap commit
    push_u32(&mut out, 0); // loader flags
    push_u32(&mut out, 16); // number of rva and sizes

    // Data directories. Only imports (index 1) is used.
    for index in 0..16u32 {
        if index == 1 {
            push_u32(&mut out, import_rva);
            push_u32(&mut out, 40); // one descriptor plus its null terminator
        } else {
            push_u32(&mut out, 0);
            push_u32(&mut out, 0);
        }
    }

    // Section headers: .text (code, read/execute) then .data (read/write).
    let mut text_section = [0u8; 40];
    text_section[0..8].copy_from_slice(b".text\0\0\0");
    text_section[8..12].copy_from_slice(&(text.len() as u32).to_le_bytes());
    text_section[12..16].copy_from_slice(&TEXT_RVA.to_le_bytes());
    text_section[16..20].copy_from_slice(&text_raw.to_le_bytes());
    text_section[20..24].copy_from_slice(&TEXT_RAW.to_le_bytes());
    text_section[36..40].copy_from_slice(&0x6000_0020u32.to_le_bytes());

    let mut data_section = [0u8; 40];
    data_section[0..8].copy_from_slice(b".data\0\0\0");
    data_section[8..12].copy_from_slice(&(data.len() as u32).to_le_bytes());
    data_section[12..16].copy_from_slice(&DATA_RVA.to_le_bytes());
    data_section[16..20].copy_from_slice(&data_raw.to_le_bytes());
    data_section[20..24].copy_from_slice(&(TEXT_RAW + text_raw).to_le_bytes());
    data_section[36..40].copy_from_slice(&0xC000_0040u32.to_le_bytes());

    out.extend_from_slice(&text_section);
    out.extend_from_slice(&data_section);
    out.resize(HEADER_SIZE as usize, 0);

    // Section contents at their raw offsets.
    out.resize(HEADER_SIZE as usize + text_raw as usize, 0);
    out[HEADER_SIZE as usize..HEADER_SIZE as usize + text.len()].copy_from_slice(text);
    let data_start = HEADER_SIZE as usize + text_raw as usize;
    out.resize(data_start + data_raw as usize, 0);
    out[data_start..data_start + data.len()].copy_from_slice(data);
    out
}

fn push_u8(out: &mut Vec<u8>, value: u8) {
    out.push(value);
}

fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}
