//! Tests to verify that our dependencies (glsl and embive) are working correctly.

mod test_glsl_parser;

#[test]
fn test_embive() {
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    // Run the test in a separate thread with a timeout
    let (tx, rx) = mpsc::channel();
    
    let handle = thread::spawn(move || {
        let result = run_embive_test();
        let _ = tx.send(result);
    });

    // Wait for the test to complete with a 500ms timeout
    match rx.recv_timeout(Duration::from_millis(500)) {
        Ok(Ok(())) => {} // Success
        Ok(Err(e)) => panic!("Test failed: {}", e),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            panic!("Test timed out after 500ms - possible infinite loop or hang");
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            panic!("Test thread disconnected unexpectedly");
        }
    }

    // Wait for thread to finish (should be quick since we got the result)
    let _ = handle.join();
}

fn run_embive_test() -> Result<(), String> {
    use core::num::NonZeroI32;
    use embive::interpreter::{
        memory::{Memory, SliceMemory, RAM_OFFSET},
        Error, Interpreter, State, SYSCALL_ARGS,
    };
    use embive::transpiler::transpile_elf;

    // Find workspace root
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .ok_or_else(|| "Could not find workspace root".to_string())?;
    
    let elf_path = workspace_root
        .join("target/riscv32imac-unknown-none-elf/debug/embive-program");
    
    // Build the program if it doesn't exist
    if !elf_path.exists() {
        std::process::Command::new("cargo")
            .args(&["build", "--package", "embive-program", "--target", "riscv32imac-unknown-none-elf"])
            .current_dir(workspace_root)
            .output()
            .map_err(|e| format!("Failed to build embive-program: {}", e))?;
    }
    
    let elf_data = std::fs::read(&elf_path)
        .map_err(|e| format!("Failed to read ELF file {:?}: {}", elf_path, e))?;

    // Transpile ELF to embive bytecode
    // The transpiler writes sections relative to entry point (0x00000000)
    // ROM sections (< RAM_OFFSET) go in code section, RAM sections (>= RAM_OFFSET) go in RAM
    let mut combined = vec![0u8; 64 * 1024];
    let binary_size = transpile_elf(&elf_data, &mut combined)
        .map_err(|e| format!("Failed to transpile ELF: {:?}", e))?;
    
    // Split ROM (low addresses) and RAM (high addresses)
    let code_size = RAM_OFFSET.min(binary_size as u32) as usize;
    let code_copy_len = code_size.min(4096);
    let mut code_vec = vec![0u8; code_copy_len.max(4096)];
    code_vec[..code_copy_len].copy_from_slice(&combined[..code_copy_len]);
    
    let mut ram = [0u8; 32 * 1024];
    if binary_size > code_size {
        let ram_offset_in_combined = code_size;
        let ram_size = (binary_size - ram_offset_in_combined).min(ram.len());
        ram[..ram_size].copy_from_slice(&combined[ram_offset_in_combined..ram_offset_in_combined + ram_size]);
    }

    let mut memory = SliceMemory::new(&code_vec, &mut ram);
    let mut interpreter = Interpreter::new(&mut memory, 0);
    // Entry point is 0x00000000, code starts there (in code section)
    interpreter.program_counter = 0;

    // Syscall handler: syscall 1 adds two numbers
    fn syscall<M: Memory>(
        nr: i32,
        args: &[i32; SYSCALL_ARGS],
        _memory: &mut M,
    ) -> Result<Result<i32, NonZeroI32>, Error> {
        match nr {
            1 => Ok(Ok(args[0] + args[1])),
            _ => Err(Error::Custom("Unknown syscall")),
        }
    }

    // Run the program (exactly like embive examples)
    loop {
        match interpreter.run().map_err(|e| format!("Interpreter error: {:?}", e))? {
            State::Running => {}
            State::Called => {
                interpreter.syscall(&mut syscall).map_err(|e| format!("Syscall error: {:?}", e))?;
            }
            State::Waiting => {}
            State::Halted => return Ok(()),
        }
    }
}
