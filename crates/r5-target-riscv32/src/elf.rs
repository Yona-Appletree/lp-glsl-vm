//! ELF generation for RISC-V 32-bit.

use alloc::vec::Vec;

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
