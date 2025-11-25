use alloc::vec::Vec;

use embive::transpiler::transpile_elf;
use embive_runtime::syscall;

/// Create a minimal ELF32 file containing RISC-V instructions
///
/// This creates a valid ELF32 RISC-V executable file with:
/// - ELF header
/// - Program header (PT_LOAD, executable)
/// - Section headers (null + .text)
/// - String table
/// - Code section with the provided instructions
fn create_minimal_elf(code: &[u8]) -> Vec<u8> {
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
    const SHT_STRTAB: u32 = 3;
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

/// JIT experiment: generate RISC-V add function, transpile it, and execute it
pub fn jit_add_experiment() {
    println!("[guest] ===== JIT EXPERIMENT START =====");
    println!("[guest] Step 1: Starting JIT experiment...");

    // Generate raw RISC-V instructions
    // add a0, a0, a1 - adds a1 to a0, result in a0
    // Encoding: 0x00b50533
    let add_inst: u32 = 0x00b50533;

    // jalr zero, 0(ra) - return (jump to return address)
    // Encoding: 0x00008067
    let ret_inst: u32 = 0x00008067;

    println!(
        "[guest] Step 2: Generated instruction encodings - add: 0x{:08x}, ret: 0x{:08x}",
        add_inst, ret_inst
    );

    // Convert to bytes (little-endian)
    let mut riscv_code = Vec::new();
    riscv_code.extend_from_slice(&add_inst.to_le_bytes());
    riscv_code.extend_from_slice(&ret_inst.to_le_bytes());

    println!(
        "[guest] Step 3: Generated {} bytes of RISC-V code",
        riscv_code.len()
    );
    println!("[guest] Step 3: Code bytes: {:02x?}", riscv_code);

    // Create minimal ELF file
    println!("[guest] Step 4: Creating minimal ELF file...");
    let elf_data = create_minimal_elf(&riscv_code);
    println!(
        "[guest] Step 4: Created ELF file ({} bytes)",
        elf_data.len()
    );
    println!("[guest] Step 4: ELF header magic: {:02x?}", &elf_data[0..4]);

    // Allocate buffer for transpiled output
    const OUTPUT_SIZE: usize = 4096;
    let mut output_buffer = Vec::with_capacity(OUTPUT_SIZE);
    output_buffer.resize(OUTPUT_SIZE, 0u8);
    println!(
        "[guest] Step 5: Allocated {} byte output buffer",
        OUTPUT_SIZE
    );

    // Transpile ELF to embive bytecode
    println!("[guest] Step 6: Transpiling ELF to embive bytecode...");
    let transpiled_size = match transpile_elf(&elf_data, &mut output_buffer) {
        Ok(size) => {
            println!("[guest] Step 6: Transpilation successful!");
            size
        }
        Err(e) => {
            println!("[guest] Step 6: FAILED to transpile ELF: {:?}", e);
            println!("[guest] ===== JIT EXPERIMENT END (FAILED) =====");
            return;
        }
    };

    println!(
        "[guest] Step 7: Transpiled to {} bytes of embive bytecode",
        transpiled_size
    );
    println!(
        "[guest] Step 7: First 16 bytes of transpiled code: {:02x?}",
        &output_buffer[0..transpiled_size.min(16)]
    );

    // Get pointer to the transpiled code (starts at offset 0 in output buffer)
    // Note: output_buffer must stay alive during the function call
    let code_ptr = output_buffer.as_ptr();
    println!("[guest] Step 8: Got code pointer: {:p}", code_ptr);

    // Cast to function pointer
    // Function signature: extern "C" fn(i32, i32) -> i32
    // Args: a0, a1 (RISC-V calling convention)
    // Return: a0
    type AddFunc = extern "C" fn(i32, i32) -> i32;
    println!("[guest] Step 9: Casting to function pointer...");
    let add_func: AddFunc = unsafe { core::mem::transmute(code_ptr) };
    println!(
        "[guest] Step 9: Function pointer created: {:p}",
        add_func as *const ()
    );

    // Call the function with test values
    // output_buffer stays in scope here, so it's safe to call
    let a = 5;
    let b = 10;
    println!(
        "[guest] Step 10: About to call JIT function: add({}, {})",
        a, b
    );
    println!("[guest] Step 10: Expected result: {}", a + b);

    println!("[guest] Step 10: Calling function now...");
    let result = add_func(a, b);
    println!("[guest] Step 10: Function call completed!");

    println!("[guest] Step 11: JIT function returned: {}", result);
    println!("[guest] Step 11: Expected: {}, Got: {}", a + b, result);

    if result == a + b {
        println!("[guest] ===== JIT EXPERIMENT SUCCESS! =====");
        // Signal completion with the JIT result
        let _ = syscall(0, &[result, 0, 0, 0, 0, 0, 0]);
    } else {
        println!("[guest] ===== JIT EXPERIMENT FAILED (wrong result) =====");
        // Signal failure with -1
        let _ = syscall(0, &[-1, 0, 0, 0, 0, 0, 0]);
    }
}
