//! ELF generation for RISC-V 32-bit.

use alloc::{format, string::String, vec::Vec};

use riscv32_encoder::disassemble_code;

/// Generate a minimal ELF32 file containing RISC-V instructions.
///
/// This creates a valid ELF32 RISC-V executable file with:
/// - ELF header
/// - Program header (PT_LOAD, executable)
/// - Section headers (null + .text)
/// - String table
/// - Code section with the provided instructions
pub fn generate_elf(code: &[u8]) -> Vec<u8> {
    // ELF constants
    const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];
    const ELFCLASS32: u8 = 1;
    const ELFDATA2LSB: u8 = 1;
    const EV_CURRENT: u8 = 1;
    const ET_EXEC: u16 = 2;
    const EM_RISCV: u16 = 243;
    const PT_LOAD: u32 = 1;
    const PF_X: u32 = 1;
    const PF_R: u32 = 4;
    const SHT_PROGBITS: u32 = 1;
    const SHF_ALLOC: u32 = 2;
    const SHF_EXECINSTR: u32 = 4;
    const SHT_STRTAB: u32 = 3;

    // Calculate offsets
    let elf_header_size = 52;
    let program_header_size = 32;
    let section_header_size = 40;
    let num_sections = 3; // null + .text + .shstrtab

    let program_header_offset = elf_header_size;
    let section_header_offset = program_header_offset + program_header_size;
    let code_offset = section_header_offset + (num_sections * section_header_size);
    let string_table_offset = code_offset + code.len();

    let mut elf = Vec::new();

    // ELF Header (52 bytes)
    elf.extend_from_slice(&ELF_MAGIC);
    elf.push(ELFCLASS32); // e_ident[EI_CLASS]
    elf.push(ELFDATA2LSB); // e_ident[EI_DATA]
    elf.push(EV_CURRENT); // e_ident[EI_VERSION]
    elf.push(0); // e_ident[EI_OSABI]
    elf.extend_from_slice(&[0; 8]); // e_ident[EI_PAD]
    elf.extend_from_slice(&(ET_EXEC as u16).to_le_bytes()); // e_type
    elf.extend_from_slice(&(EM_RISCV as u16).to_le_bytes()); // e_machine
    elf.extend_from_slice(&(EV_CURRENT as u32).to_le_bytes()); // e_version
    elf.extend_from_slice(&0u32.to_le_bytes()); // e_entry (0x0)
    elf.extend_from_slice(&(program_header_offset as u32).to_le_bytes()); // e_phoff
    elf.extend_from_slice(&(section_header_offset as u32).to_le_bytes()); // e_shoff
    elf.extend_from_slice(&0u32.to_le_bytes()); // e_flags
    elf.extend_from_slice(&(elf_header_size as u16).to_le_bytes()); // e_ehsize
    elf.extend_from_slice(&(program_header_size as u16).to_le_bytes()); // e_phentsize
    elf.extend_from_slice(&1u16.to_le_bytes()); // e_phnum (1 program header)
    elf.extend_from_slice(&(section_header_size as u16).to_le_bytes()); // e_shentsize
    elf.extend_from_slice(&(num_sections as u16).to_le_bytes()); // e_shnum
    elf.extend_from_slice(&2u16.to_le_bytes()); // e_shstrndx (string table section index = 2)

    // Program Header (32 bytes)
    elf.extend_from_slice(&(PT_LOAD as u32).to_le_bytes()); // p_type
    elf.extend_from_slice(&(code_offset as u32).to_le_bytes()); // p_offset
    elf.extend_from_slice(&0u32.to_le_bytes()); // p_vaddr (0x0)
    elf.extend_from_slice(&0u32.to_le_bytes()); // p_paddr (0x0)
    elf.extend_from_slice(&(code.len() as u32).to_le_bytes()); // p_filesz
    elf.extend_from_slice(&(code.len() as u32).to_le_bytes()); // p_memsz
    elf.extend_from_slice(&(PF_X | PF_R).to_le_bytes()); // p_flags
    elf.extend_from_slice(&4u32.to_le_bytes()); // p_align

    // Section Headers (40 bytes each)
    // Null section (all zeros)
    elf.extend_from_slice(&[0u8; 40]);

    // .text section
    elf.extend_from_slice(&1u32.to_le_bytes()); // sh_name (offset 1 in string table = ".text")
    elf.extend_from_slice(&(SHT_PROGBITS as u32).to_le_bytes()); // sh_type
    elf.extend_from_slice(&(SHF_ALLOC | SHF_EXECINSTR).to_le_bytes()); // sh_flags
    elf.extend_from_slice(&0u32.to_le_bytes()); // sh_addr (0x0)
    elf.extend_from_slice(&(code_offset as u32).to_le_bytes()); // sh_offset
    elf.extend_from_slice(&(code.len() as u32).to_le_bytes()); // sh_size
    elf.extend_from_slice(&0u32.to_le_bytes()); // sh_link
    elf.extend_from_slice(&0u32.to_le_bytes()); // sh_info
    elf.extend_from_slice(&4u32.to_le_bytes()); // sh_addralign
    elf.extend_from_slice(&0u32.to_le_bytes()); // sh_entsize

    // .shstrtab section (string table)
    elf.extend_from_slice(&7u32.to_le_bytes()); // sh_name (offset 7 in string table = ".shstrtab")
    elf.extend_from_slice(&(SHT_STRTAB as u32).to_le_bytes()); // sh_type
    elf.extend_from_slice(&0u32.to_le_bytes()); // sh_flags
    elf.extend_from_slice(&0u32.to_le_bytes()); // sh_addr
    elf.extend_from_slice(&(string_table_offset as u32).to_le_bytes()); // sh_offset
    elf.extend_from_slice(&15u32.to_le_bytes()); // sh_size ("\0.text\0.shstrtab\0" = 15 bytes)
    elf.extend_from_slice(&0u32.to_le_bytes()); // sh_link
    elf.extend_from_slice(&0u32.to_le_bytes()); // sh_info
    elf.extend_from_slice(&1u32.to_le_bytes()); // sh_addralign
    elf.extend_from_slice(&0u32.to_le_bytes()); // sh_entsize

    // Code section
    elf.extend_from_slice(code);

    // String table
    elf.extend_from_slice(b"\0.text\0.shstrtab\0");

    elf
}

/// Debug an ELF file by parsing and displaying its structure.
///
/// Returns a formatted string showing:
/// - ELF header information
/// - Program headers
/// - Section headers
/// - Disassembled code from .text section
pub fn debug_elf(elf_data: &[u8]) -> String {
    if elf_data.len() < 52 {
        return format!("Invalid ELF: too small ({} bytes)", elf_data.len());
    }

    let mut result = String::new();

    // Parse ELF header
    let magic = &elf_data[0..4];
    if magic != &[0x7f, b'E', b'L', b'F'] {
        return format!("Invalid ELF magic: {:02x?}", magic);
    }

    let class = elf_data[4];
    let data = elf_data[5];
    let version = elf_data[6];
    let e_type = u16::from_le_bytes([elf_data[16], elf_data[17]]);
    let e_machine = u16::from_le_bytes([elf_data[18], elf_data[19]]);
    let e_version = u32::from_le_bytes([
        elf_data[20],
        elf_data[21],
        elf_data[22],
        elf_data[23],
    ]);
    let e_entry = u32::from_le_bytes([
        elf_data[24],
        elf_data[25],
        elf_data[26],
        elf_data[27],
    ]);
    let e_phoff = u32::from_le_bytes([
        elf_data[28],
        elf_data[29],
        elf_data[30],
        elf_data[31],
    ]);
    let e_shoff = u32::from_le_bytes([
        elf_data[32],
        elf_data[33],
        elf_data[34],
        elf_data[35],
    ]);
    let e_ehsize = u16::from_le_bytes([elf_data[40], elf_data[41]]);
    let e_phentsize = u16::from_le_bytes([elf_data[42], elf_data[43]]);
    let e_phnum = u16::from_le_bytes([elf_data[44], elf_data[45]]);
    let e_shentsize = u16::from_le_bytes([elf_data[46], elf_data[47]]);
    let e_shnum = u16::from_le_bytes([elf_data[48], elf_data[49]]);
    let e_shstrndx = u16::from_le_bytes([elf_data[50], elf_data[51]]);

    result.push_str("=== ELF Header ===\n");
    result.push_str(&format!("  Magic: {:02x?}\n", magic));
    result.push_str(&format!("  Class: {} (32-bit)\n", class));
    result.push_str(&format!("  Data: {} (little-endian)\n", data));
    result.push_str(&format!("  Version: {}\n", version));
    result.push_str(&format!("  Type: {}\n", e_type));
    result.push_str(&format!("  Machine: {} (RISC-V)\n", e_machine));
    result.push_str(&format!("  Version: {}\n", e_version));
    result.push_str(&format!("  Entry point: 0x{:08x}\n", e_entry));
    result.push_str(&format!("  Program header offset: 0x{:08x}\n", e_phoff));
    result.push_str(&format!("  Section header offset: 0x{:08x}\n", e_shoff));
    result.push_str(&format!("  Header size: {}\n", e_ehsize));
    result.push_str(&format!("  Program header size: {}\n", e_phentsize));
    result.push_str(&format!("  Number of program headers: {}\n", e_phnum));
    result.push_str(&format!("  Section header size: {}\n", e_shentsize));
    result.push_str(&format!("  Number of sections: {}\n", e_shnum));
    result.push_str(&format!("  String table index: {}\n", e_shstrndx));

    // Parse program headers
    if e_phoff > 0 && e_phnum > 0 {
        result.push_str("\n=== Program Headers ===\n");
        for i in 0..e_phnum {
            let ph_offset = e_phoff as usize + (i as usize * e_phentsize as usize);
            if ph_offset + 32 > elf_data.len() {
                result.push_str(&format!("  Program header {}: out of bounds\n", i));
                continue;
            }

            let p_type = u32::from_le_bytes([
                elf_data[ph_offset],
                elf_data[ph_offset + 1],
                elf_data[ph_offset + 2],
                elf_data[ph_offset + 3],
            ]);
            let p_offset = u32::from_le_bytes([
                elf_data[ph_offset + 4],
                elf_data[ph_offset + 5],
                elf_data[ph_offset + 6],
                elf_data[ph_offset + 7],
            ]);
            let p_vaddr = u32::from_le_bytes([
                elf_data[ph_offset + 8],
                elf_data[ph_offset + 9],
                elf_data[ph_offset + 10],
                elf_data[ph_offset + 11],
            ]);
            let p_paddr = u32::from_le_bytes([
                elf_data[ph_offset + 12],
                elf_data[ph_offset + 13],
                elf_data[ph_offset + 14],
                elf_data[ph_offset + 15],
            ]);
            let p_filesz = u32::from_le_bytes([
                elf_data[ph_offset + 16],
                elf_data[ph_offset + 17],
                elf_data[ph_offset + 18],
                elf_data[ph_offset + 19],
            ]);
            let p_memsz = u32::from_le_bytes([
                elf_data[ph_offset + 20],
                elf_data[ph_offset + 21],
                elf_data[ph_offset + 22],
                elf_data[ph_offset + 23],
            ]);
            let p_flags = u32::from_le_bytes([
                elf_data[ph_offset + 24],
                elf_data[ph_offset + 25],
                elf_data[ph_offset + 26],
                elf_data[ph_offset + 27],
            ]);
            let p_align = u32::from_le_bytes([
                elf_data[ph_offset + 28],
                elf_data[ph_offset + 29],
                elf_data[ph_offset + 30],
                elf_data[ph_offset + 31],
            ]);

            let type_str = match p_type {
                1 => "PT_LOAD",
                _ => "UNKNOWN",
            };
            let flags_str = {
                let mut f = String::new();
                if (p_flags & 1u32) != 0 {
                    f.push_str("X");
                }
                if (p_flags & 2u32) != 0 {
                    f.push_str("W");
                }
                if (p_flags & 4u32) != 0 {
                    f.push_str("R");
                }
                if f.is_empty() {
                    f.push_str("-");
                }
                f
            };

            result.push_str(&format!("  {}:\n", i));
            result.push_str(&format!("    Type: {} ({})\n", type_str, p_type));
            result.push_str(&format!("    Offset: 0x{:08x}\n", p_offset));
            result.push_str(&format!("    Virtual address: 0x{:08x}\n", p_vaddr));
            result.push_str(&format!("    Physical address: 0x{:08x}\n", p_paddr));
            result.push_str(&format!("    File size: {}\n", p_filesz));
            result.push_str(&format!("    Memory size: {}\n", p_memsz));
            result.push_str(&format!("    Flags: {} (0x{:x})\n", flags_str, p_flags));
            result.push_str(&format!("    Align: {}\n", p_align));
        }
    }

    // Parse section headers and find .text section
    let mut text_offset = 0;
    let mut text_size = 0;
    if e_shoff > 0 && e_shnum > 0 {
        result.push_str("\n=== Section Headers ===\n");
        for i in 0..e_shnum {
            let sh_offset = e_shoff as usize + (i as usize * e_shentsize as usize);
            if sh_offset + 40 > elf_data.len() {
                result.push_str(&format!("  Section {}: out of bounds\n", i));
                continue;
            }

            let sh_name = u32::from_le_bytes([
                elf_data[sh_offset],
                elf_data[sh_offset + 1],
                elf_data[sh_offset + 2],
                elf_data[sh_offset + 3],
            ]);
            let sh_type = u32::from_le_bytes([
                elf_data[sh_offset + 4],
                elf_data[sh_offset + 5],
                elf_data[sh_offset + 6],
                elf_data[sh_offset + 7],
            ]);
            let sh_flags = u32::from_le_bytes([
                elf_data[sh_offset + 8],
                elf_data[sh_offset + 9],
                elf_data[sh_offset + 10],
                elf_data[sh_offset + 11],
            ]);
            let sh_addr = u32::from_le_bytes([
                elf_data[sh_offset + 12],
                elf_data[sh_offset + 13],
                elf_data[sh_offset + 14],
                elf_data[sh_offset + 15],
            ]);
            let sh_offset_val = u32::from_le_bytes([
                elf_data[sh_offset + 16],
                elf_data[sh_offset + 17],
                elf_data[sh_offset + 18],
                elf_data[sh_offset + 19],
            ]);
            let sh_size = u32::from_le_bytes([
                elf_data[sh_offset + 20],
                elf_data[sh_offset + 21],
                elf_data[sh_offset + 22],
                elf_data[sh_offset + 23],
            ]);

            // Try to get section name from string table
            let name_str = if e_shstrndx < e_shnum {
                let strtab_sh_offset = e_shoff as usize + (e_shstrndx as usize * e_shentsize as usize);
                if strtab_sh_offset + 16 <= elf_data.len() {
                    let strtab_offset = u32::from_le_bytes([
                        elf_data[strtab_sh_offset + 16],
                        elf_data[strtab_sh_offset + 17],
                        elf_data[strtab_sh_offset + 18],
                        elf_data[strtab_sh_offset + 19],
                    ]) as usize;
                    let name_offset = strtab_offset + (sh_name as usize);
                    if name_offset < elf_data.len() {
                        let name_start = name_offset;
                        let name_end = elf_data[name_start..]
                            .iter()
                            .position(|&b| b == 0)
                            .map(|p| name_start + p)
                            .unwrap_or(elf_data.len());
                        String::from(
                            core::str::from_utf8(&elf_data[name_start..name_end])
                                .unwrap_or("?")
                        )
                    } else {
                        format!("?{}", sh_name)
                    }
                } else {
                    format!("?{}", sh_name)
                }
            } else {
                format!("?{}", sh_name)
            };

            let type_str = match sh_type {
                0 => "NULL",
                1 => "PROGBITS",
                3 => "STRTAB",
                _ => "UNKNOWN",
            };

            result.push_str(&format!("  {}: {}\n", i, name_str));
            result.push_str(&format!("    Type: {} ({})\n", type_str, sh_type));
            result.push_str(&format!("    Flags: 0x{:08x}\n", sh_flags));
            result.push_str(&format!("    Address: 0x{:08x}\n", sh_addr));
            result.push_str(&format!("    Offset: 0x{:08x}\n", sh_offset_val));
            result.push_str(&format!("    Size: {}\n", sh_size));

            // Track .text section for disassembly
            if name_str == ".text" {
                text_offset = sh_offset_val as usize;
                text_size = sh_size as usize;
            }
        }
    }

    // Disassemble .text section
    if text_offset > 0 && text_size > 0 && text_offset + text_size <= elf_data.len() {
        result.push_str("\n=== Disassembled Code (.text) ===\n");
        let code = &elf_data[text_offset..text_offset + text_size];
        result.push_str(&disassemble_code(code));
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_elf() {
        // Simple test: generate ELF with empty code
        let code = [0u8; 8];
        let elf = generate_elf(&code);
        assert!(elf.len() > 0);
        // Check ELF magic
        assert_eq!(&elf[0..4], &[0x7f, b'E', b'L', b'F']);
    }
}
