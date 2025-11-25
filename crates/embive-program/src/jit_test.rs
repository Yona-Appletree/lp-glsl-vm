use alloc::vec;

use embive::transpiler::transpile_elf;
use embive_runtime::syscall;
use r5_builder::FunctionBuilder;
use r5_ir::{Signature, Type};
use r5_target_riscv32::{compile_function, generate_elf};

/// JIT experiment: generate RISC-V add function using the new compiler architecture,
/// transpile it, and execute it
pub fn jit_add_experiment() {
    println!("[guest] ===== JIT EXPERIMENT START =====");
    println!("[guest] Step 1: Building IR for add function...");

    // Build IR: fn add(a: i32, b: i32) -> i32 { a + b }
    let sig = Signature::new(alloc::vec![Type::I32, Type::I32], alloc::vec![Type::I32]);
    let mut builder = FunctionBuilder::new(sig);
    let block_idx = builder.create_block();

    // Create values for parameters and result
    // In a real implementation, parameters would come from block params
    // For now, we'll use values directly
    let a = builder.new_value();
    let b = builder.new_value();
    let result = builder.new_value();

    println!("[guest] Step 2: Adding instructions to IR...");
    {
        let mut block_builder = builder.block_builder(block_idx);
        block_builder.imul(result, a, b);
        block_builder.return_(&alloc::vec![result]);
    }

    let func = builder.finish();
    println!(
        "[guest] Step 2: IR function built with {} blocks",
        func.block_count()
    );

    // Compile IR to RISC-V code
    println!("[guest] Step 3: Compiling IR to RISC-V code...");
    let riscv_code = compile_function(&func);
    println!(
        "[guest] Step 3: Generated {} bytes of RISC-V code",
        riscv_code.len()
    );
    println!(
        "[guest] Step 3: Code bytes: {:02x?}",
        &riscv_code[0..riscv_code.len().min(16)]
    );

    // Generate ELF file
    println!("[guest] Step 4: Generating ELF file...");
    let elf_data = generate_elf(&riscv_code);
    println!(
        "[guest] Step 4: Created ELF file ({} bytes)",
        elf_data.len()
    );
    println!("[guest] Step 4: ELF header magic: {:02x?}", &elf_data[0..4]);

    // Allocate buffer for transpiled output
    const OUTPUT_SIZE: usize = 4096;
    let mut output_buffer = alloc::vec::Vec::with_capacity(OUTPUT_SIZE);
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
    type MulFunc = extern "C" fn(i32, i32) -> i32;
    println!("[guest] Step 9: Casting to function pointer...");
    let mul_func: MulFunc = unsafe { core::mem::transmute(code_ptr) };
    println!(
        "[guest] Step 9: Function pointer created: {:p}",
        mul_func as *const ()
    );

    // Call the function with test values
    // output_buffer stays in scope here, so it's safe to call
    let a = 5;
    let b = 10;
    println!(
        "[guest] Step 10: About to call JIT function: mul({}, {})",
        a, b
    );
    println!("[guest] Step 10: Expected result: {}", a * b);

    println!("[guest] Step 10: Calling function now...");
    let result = mul_func(a, b);
    println!("[guest] Step 10: Function call completed!");

    println!("[guest] Step 11: JIT function returned: {}", result);
    println!("[guest] Step 11: Expected: {}, Got: {}", a * b, result);

    if result == a * b {
        println!("[guest] ===== JIT EXPERIMENT SUCCESS! =====");
        // Signal completion with the JIT result
        let _ = syscall(0, &[result, 0, 0, 0, 0, 0, 0]);
    } else {
        println!("[guest] ===== JIT EXPERIMENT FAILED (wrong result) =====");
        // Signal failure with -1
        let _ = syscall(0, &[-1, 0, 0, 0, 0, 0, 0]);
    }
}
