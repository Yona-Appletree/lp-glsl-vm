//! Tests to verify that our dependencies (glsl and embive) are working correctly.

use glsl::parser::Parse;
use glsl::syntax::TranslationUnit;

#[test]
fn test_glsl_parser() {
    // Test that we can parse a simple GLSL fragment shader
    let glsl_code = r#"
        void main() {
            gl_FragColor = vec4(1.0, 0.5, 0.25, 1.0);
        }
    "#;

    let result = TranslationUnit::parse(glsl_code);
    assert!(result.is_ok(), "GLSL parsing failed: {:?}", result.err());
    
    let translation_unit = result.unwrap();
    // TranslationUnit contains a NonEmpty<ExternalDeclaration>, which always has at least one element
    // NonEmpty wraps a Vec, so we access it via .0
    assert!(translation_unit.0.0.len() > 0, "Translation unit should not be empty");
}

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

    // First, build the embive-program if it doesn't exist
    let elf_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/riscv32imac-unknown-none-elf/debug/embive-program");
    
    // Build the program if it doesn't exist
    if !elf_path.exists() {
        std::process::Command::new("cargo")
            .args(&["build", "--package", "embive-program", "--target", "riscv32imac-unknown-none-elf"])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()
            .map_err(|e| format!("Failed to build embive-program: {}", e))?;
    }
    
    let elf_data = std::fs::read(&elf_path)
        .map_err(|e| format!("Failed to read ELF file {:?}: {}", elf_path, e))?;

    // Transpile ELF to embive bytecode
    let mut code = [0u8; 16384];
    transpile_elf(&elf_data, &mut code)
        .map_err(|e| format!("Failed to transpile ELF: {:?}", e))?;

    // Initialize memory
    let mut ram = [0u8; 16384];
    let mut memory = SliceMemory::new(&code, &mut ram);

    // Create interpreter
    let mut interpreter = Interpreter::new(&mut memory, 0);

    // Set program counter to RAM_OFFSET (where transpiled code is)
    interpreter.program_counter = RAM_OFFSET;

    // Define syscall handler that adds two numbers
    // Syscall 1: add args[0] + args[1]
    fn syscall<M: Memory>(
        nr: i32,
        args: &[i32; SYSCALL_ARGS],
        _memory: &mut M,
    ) -> Result<Result<i32, NonZeroI32>, Error> {
        match nr {
            1 => {
                // Add two numbers
                Ok(Ok(args[0] + args[1]))
            }
            _ => Err(Error::Custom("Unknown syscall")),
        }
    }

    // Run the program
    let mut iterations = 0;
    const MAX_ITERATIONS: u64 = 10000;
    
    loop {
        iterations += 1;
        if iterations > MAX_ITERATIONS {
            return Err(format!("Test exceeded maximum iterations ({}), possible infinite loop", MAX_ITERATIONS));
        }
        
        match interpreter.run().map_err(|e| format!("Interpreter error: {:?}", e))? {
            State::Running => {}
            State::Called => {
                // Handle syscall
                interpreter.syscall(&mut syscall).map_err(|e| format!("Syscall error: {:?}", e))?;
            }
            State::Waiting => {
                // No interrupts needed for this simple test
                return Err("Unexpected wait state".to_string());
            }
            State::Halted => {
                // Program completed successfully
                // The program should have called syscall(1, [5, 10, ...]) which returns 15
                return Ok(());
            }
        }
    }
}
