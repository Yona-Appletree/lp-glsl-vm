use alloc::vec::Vec;

use embive::transpiler::transpile_elf;
use embive_runtime::syscall;
use riscv_shared::build_and_compile_fib;

/// JIT experiment: generate RISC-V fibonacci function using the new compiler architecture,
/// transpile it, and execute it via embive VM
pub fn jit_add_experiment() {
    println!("[guest] ===== JIT EXPERIMENT START =====");
    println!("[guest] Step 1: Building IR and compiling...");

    // Build and compile using shared code
    let jit_result = build_and_compile_fib();

    println!(
        "[guest] Step 2: Generated {} bytes of RISC-V code",
        jit_result.code.len()
    );
    println!(
        "[guest] Step 2: Code bytes: {:02x?}",
        &jit_result.code[0..jit_result.code.len().min(16)]
    );
    println!(
        "[guest] Step 3: ELF file size: {} bytes",
        jit_result.elf.len()
    );
    println!(
        "[guest] Step 3: ELF header magic: {:02x?}",
        &jit_result.elf[0..4]
    );

    // Allocate buffer for transpiled output
    const OUTPUT_SIZE: usize = 4096;
    let mut output_buffer = Vec::with_capacity(OUTPUT_SIZE);
    output_buffer.resize(OUTPUT_SIZE, 0u8);
    println!(
        "[guest] Step 4: Allocated {} byte output buffer",
        OUTPUT_SIZE
    );

    // Transpile ELF to embive bytecode
    println!("[guest] Step 5: Transpiling ELF to embive bytecode...");
    let transpiled_size = match transpile_elf(&jit_result.elf, &mut output_buffer) {
        Ok(size) => {
            println!("[guest] Step 5: Transpilation successful!");
            size
        }
        Err(e) => {
            println!("[guest] Step 5: FAILED to transpile ELF: {:?}", e);
            println!("[guest] ===== JIT EXPERIMENT END (FAILED) =====");
            return;
        }
    };

    println!(
        "[guest] Step 6: Transpiled to {} bytes of embive bytecode",
        transpiled_size
    );
    println!(
        "[guest] Step 6: First 16 bytes of transpiled code: {:02x?}",
        &output_buffer[0..transpiled_size.min(16)]
    );

    // Get pointer to the transpiled code
    let code_ptr = output_buffer.as_ptr();
    println!("[guest] Step 7: Got code pointer: {:p}", code_ptr);

    // Cast to function pointer (bootstrap function that calls fib(10))
    type BootstrapFunc = extern "C" fn() -> i32;
    println!("[guest] Step 8: Casting to function pointer...");
    let bootstrap_func: BootstrapFunc = unsafe { core::mem::transmute(code_ptr) };
    println!(
        "[guest] Step 8: Function pointer created: {:p}",
        bootstrap_func as *const ()
    );

    // Call the bootstrap function which will call fib(10) and return the result
    println!("[guest] Step 9: About to call bootstrap function (which calls fib(10))");
    println!("[guest] Step 9: Expected result: 55 (fib(10))");

    println!("[guest] Step 9: Calling function now...");
    let result = bootstrap_func();
    println!("[guest] Step 9: Function call completed!");

    println!("[guest] Step 10: Bootstrap function returned: {}", result);
    println!("[guest] Step 10: Expected: 55, Got: {}", result);

    if result == 55 {
        println!("[guest] ===== JIT EXPERIMENT SUCCESS! =====");
        // Signal completion with the JIT result
        let _ = syscall(0, &[result, 0, 0, 0, 0, 0, 0]);
    } else {
        println!("[guest] ===== JIT EXPERIMENT FAILED (wrong result) =====");
        // Signal failure with -1
        let _ = syscall(0, &[-1, 0, 0, 0, 0, 0, 0]);
    }
}
