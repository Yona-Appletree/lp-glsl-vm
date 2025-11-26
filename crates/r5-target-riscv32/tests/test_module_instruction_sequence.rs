#[test]
fn test_module_instruction_sequence() {
    // Test that the compiled module executes correctly and produces expected results
    let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
call %main() -> v0
syscall 0(v0)
halt
}

function %helper() -> i32 {
block0:
v0 = iconst 42
return v0
}

function %main() -> i32 {
block0:
call %helper() -> v0
return v0
}
}"#;

    use r5_ir::parse_module;
    use r5_target_riscv32::compile_module_to_insts;
    use riscv32_emulator::{debug_riscv32_bytes, LogLevel, Riscv32Emulator};
    use riscv32_encoder::{disassemble_code, Gpr};

    let module = parse_module(ir).expect("Failed to parse module");
    let compiled = compile_module_to_insts(&module).expect("Compilation failed");

    // Print assembly from CompiledModule for debugging
    eprintln!("=== Assembly from CompiledModule ===");
    eprintln!("{}", compiled);

    // Compile to bytes and disassemble
    let bytes = compiled.to_bytes().expect("Failed to convert to bytes");
    eprintln!("=== Disassembled from bytes ===");
    eprintln!("{}", disassemble_code(&bytes));

    // Run in emulator until ECALL (syscall)
    // SP is initialized to 0x80001000 (RAM_OFFSET + 0x1000)
    let mut emu = Riscv32Emulator::new(bytes.clone(), vec![0; 1024 * 1024])
        .with_log_level(LogLevel::Instructions);

    // Run until syscall is encountered
    let syscall_info = match emu.run_until_ecall() {
        Ok(info) => info,
        Err(e) => {
            // Print detailed error information
            eprintln!("\n=== Execution Error ===");
            eprintln!("Error: {}", e);
            eprintln!("PC: 0x{:08x}", e.pc());
            eprintln!("\nCode length: {} bytes (0x{:x})", bytes.len(), bytes.len());
            eprintln!("Last 10 logs:");
            for log in emu.get_logs().iter().rev().take(10) {
                eprintln!("{}", log);
            }
            panic!("Execution failed before syscall: {}", e);
        }
    };

    // Verify syscall number is 0
    assert_eq!(syscall_info.number, 0, "Expected syscall number 0");

    // Verify syscall argument (a0) is 42 (return value from main)
    assert_eq!(
        syscall_info.args[0], 42,
        "Expected syscall argument (a0) to be 42"
    );

    // Verify a0 register contains 42
    assert_eq!(
        emu.get_register(Gpr::A0),
        42,
        "Expected a0 register to contain 42 after main() call"
    );

    // Continue execution to EBREAK (halt)
    match emu.step() {
        Ok(riscv32_emulator::StepResult::Halted) => {
            // Successfully halted
        }
        Ok(other) => {
            panic!("Expected Halted after syscall, got {:?}", other);
        }
        Err(e) => {
            panic!("Error after syscall: {}", e);
        }
    }

    eprintln!("=== Test passed: Module executed correctly ===");
}
