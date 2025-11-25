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
        memory::{SliceMemory, RAM_OFFSET},
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
    
    // Build the program (cargo handles dependency tracking automatically)
    // If embive-program or embive-runtime changes, cargo will rebuild automatically
    std::process::Command::new("cargo")
        .args(&["build", "--package", "embive-program", "--target", "riscv32imac-unknown-none-elf"])
        .current_dir(workspace_root)
        .output()
        .map_err(|e| format!("Failed to build embive-program: {}", e))?;
    
    let elf_data = std::fs::read(&elf_path)
        .map_err(|e| format!("Failed to read ELF file {:?}: {}", elf_path, e))?;

    // Transpile ELF to embive bytecode
    // The transpiler writes sections relative to entry point (0x00000000)
    // ROM sections (< RAM_OFFSET) go in code section, RAM sections (>= RAM_OFFSET) go in RAM
    // Use 4MB buffer to ensure we can handle large binaries
    const MAX_BINARY_SIZE: usize = 4 * 1024 * 1024;
    let mut combined = vec![0u8; MAX_BINARY_SIZE];
    let binary_size = transpile_elf(&elf_data, &mut combined)
        .map_err(|e| format!("Failed to transpile ELF: {:?}", e))?;
    
    // Split ROM (low addresses) and RAM (high addresses)
    let code_size = RAM_OFFSET.min(binary_size as u32) as usize;
    // Load the full code section - allocate enough space for the entire code section
    // Ensure minimum size to avoid zero-sized allocation issues
    let mut code_vec = vec![0u8; code_size.max(1)];
    if code_size > 0 {
        code_vec[..code_size].copy_from_slice(&combined[..code_size]);
    }
    
    // Allocate 4MB for RAM section
    const RAM_SIZE: usize = 4 * 1024 * 1024;
    let mut ram = vec![0u8; RAM_SIZE];
    if binary_size > code_size {
        let ram_offset_in_combined = code_size;
        let ram_size = (binary_size - ram_offset_in_combined).min(ram.len());
        ram[..ram_size].copy_from_slice(&combined[ram_offset_in_combined..ram_offset_in_combined + ram_size]);
    }

    // Store raw pointers to memory slices for reading in syscall handler
    // This is safe because we only read from these pointers and the memory outlives the closure
    let code_vec_ptr = code_vec.as_ptr();
    let code_vec_len = code_vec.len();
    let ram_ptr = ram.as_ptr();
    let ram_len = ram.len();

    let mut memory = SliceMemory::new(&code_vec, &mut ram);
    let mut interpreter = Interpreter::new(&mut memory, 0);
    // Entry point is 0x00000000, code starts there (in code section)
    interpreter.program_counter = 0;

    // Track syscall invocations to verify it's actually being called
    use std::cell::Cell;
    let syscall_count = Cell::new(0);
    let syscall_args = Cell::new((0, 0));
    let syscall_result = Cell::new(None);

    // Helper function to read bytes from guest memory
    // Addresses < RAM_OFFSET are in code_vec (ROM), addresses >= RAM_OFFSET are in ram
    let read_memory = |addr: u32, len: usize| -> Result<Vec<u8>, Error> {
        let mut buf = vec![0u8; len];
        unsafe {
            if addr < RAM_OFFSET {
                // Read from code section (ROM)
                let offset = addr as usize;
                if offset + len > code_vec_len {
                    return Err(Error::Custom("Address out of bounds in code section"));
                }
                core::ptr::copy_nonoverlapping(
                    code_vec_ptr.add(offset),
                    buf.as_mut_ptr(),
                    len,
                );
            } else {
                // Read from RAM section
                let offset = (addr - RAM_OFFSET) as usize;
                if offset + len > ram_len {
                    return Err(Error::Custom("Address out of bounds in RAM section"));
                }
                core::ptr::copy_nonoverlapping(
                    ram_ptr.add(offset),
                    buf.as_mut_ptr(),
                    len,
                );
            }
        }
        Ok(buf)
    };

    // Syscall handler: syscall 1 adds two numbers, syscall 2 writes strings
    let mut syscall = |nr: i32, args: &[i32; SYSCALL_ARGS], _memory: &mut _| -> Result<Result<i32, NonZeroI32>, Error> {
        match nr {
            1 => {
                let count = syscall_count.get();
                syscall_count.set(count + 1);
                syscall_args.set((args[0], args[1]));
                let result = args[0] + args[1];
                syscall_result.set(Some(result));
                println!("syscall add2(1): {} + {} = {}", args[0], args[1], result);
                Ok(Ok(result))
            },
            2 => {
                // Syscall 2: Write string to host
                // args[0] = pointer to string (as i32)
                // args[1] = length of string
                let ptr = args[0] as u32;
                let len = args[1] as usize;
                
                // Read string from guest memory
                let buf = read_memory(ptr, len)?;
                
                // Convert to string and print
                let s = core::str::from_utf8(&buf)
                    .map_err(|_| Error::Custom("Invalid UTF-8 string"))?;
                print!("{}", s);
                
                Ok(Ok(0))
            },
            _ => Err(Error::Custom("Unknown syscall")),
        }
    };

    // Run the program (exactly like embive examples)
    loop {
        match interpreter.run().map_err(|e| format!("Interpreter error: {:?}", e))? {
            State::Running => {}
            State::Called => {
                interpreter.syscall(&mut syscall).map_err(|e| format!("Syscall error: {:?}", e))?;
            }
            State::Waiting => {}
            State::Halted => break,
        }
    }

    // Verify the syscall was actually called
    let count = syscall_count.get();
    if count == 0 {
        return Err("Syscall was never called".to_string());
    }
    
    // Verify the syscall was called exactly once
    if count != 1 {
        return Err(format!("Expected syscall to be called once, but it was called {} times", count));
    }

    // Verify the arguments passed to the syscall
    let (arg0, arg1) = syscall_args.get();
    if arg0 != 5 || arg1 != 10 {
        return Err(format!("Expected syscall args (5, 10), but got ({}, {})", arg0, arg1));
    }

    // Verify the return value
    let result = syscall_result.get().ok_or("Syscall result was not set")?;
    if result != 15 {
        return Err(format!("Expected syscall result 15, but got {}", result));
    }

    println!("✅ Syscall verification passed: called {} time(s) with args ({}, {}) = {}", count, arg0, arg1, result);
    Ok(())
}
