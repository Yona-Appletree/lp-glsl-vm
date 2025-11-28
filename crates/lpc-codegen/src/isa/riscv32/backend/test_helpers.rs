//! Helper functions for testing IR code compilation and execution.

extern crate alloc;

use alloc::{format, string::String, vec};

use super::compile_module_to_insts;
use crate::{
    disassemble_code,
    emu::{EmulatorError, LogLevel, Riscv32Emulator, StepResult},
    Gpr,
};

/// Create an emu from IR code.
pub fn debug_ir(ir: &str) -> Result<Riscv32Emulator, EmulatorError> {
    debug_ir_with_ram(ir, 1024 * 1024)
}

/// Create an emu from IR code with specified RAM size.
pub fn debug_ir_with_ram(ir: &str, ram_size: usize) -> Result<Riscv32Emulator, EmulatorError> {
    let module = lpc_lpir::parse_module(ir).map_err(|e| EmulatorError::InvalidInstruction {
        pc: 0,
        instruction: 0,
        reason: format!("IR parse error: {}", e),
        regs: [0; 32],
    })?;

    let compiled =
        compile_module_to_insts(&module).map_err(|e| EmulatorError::InvalidInstruction {
            pc: 0,
            instruction: 0,
            reason: format!("Compilation error: {}", e),
            regs: [0; 32],
        })?;

    let bytes = compiled
        .to_bytes()
        .map_err(|e| EmulatorError::InvalidInstruction {
            pc: 0,
            instruction: 0,
            reason: format!("Failed to convert to bytes: {}", e),
            regs: [0; 32],
        })?;

    let mut emu =
        Riscv32Emulator::new(bytes, vec![0; ram_size]).with_log_level(LogLevel::Instructions);
    // Initialize stack pointer to a valid address (RAM starts at 0x80000000)
    emu.set_register(Gpr::Sp, 0x80001000u32 as i32);
    Ok(emu)
}

/// Format error with IR source, disassembly, and logs.
fn format_ir_error(emu: &Riscv32Emulator, error: &EmulatorError, code: &[u8], ir: &str) -> String {
    let mut result = String::new();
    let error_pc = error.pc();

    result.push_str("=== IR Execution Error ===\n\n");
    result.push_str("IR Source:\n");
    result.push_str(ir);
    result.push_str("\n\n");
    result.push_str(&format!("Error: {}\n", error));
    result.push_str(&format!("PC: 0x{:08x}\n\n", error_pc));

    // Add disassembly
    result.push_str("=== Generated Assembly ===\n");
    result.push_str(&disassemble_code(code));
    result.push_str("\n");

    result.push_str(&emu.format_debug_info(Some(error_pc), 10));

    result
}

/// Format register mismatch with IR source, disassembly, and logs.
fn format_ir_register_mismatch(
    emu: &Riscv32Emulator,
    code: &[u8],
    ir: &str,
    reg: Gpr,
    expected: i32,
    actual: i32,
) -> String {
    let mut result = String::new();

    result.push_str("=== Register Mismatch ===\n\n");
    result.push_str("IR Source:\n");
    result.push_str(ir);
    result.push_str("\n\n");
    result.push_str(&format!(
        "Register {:?} mismatch: expected {}, got {}\n\n",
        reg, expected, actual
    ));

    // Add disassembly
    result.push_str("=== Generated Assembly ===\n");
    result.push_str(&disassemble_code(code));
    result.push_str("\n");

    result.push_str(&emu.format_debug_info(None, 20));

    result
}

/// Expect IR code to run successfully until EBREAK, returning the emu.
pub fn expect_ir_ok(ir: &str) -> Riscv32Emulator {
    let mut emu = debug_ir(ir).expect("Failed to compile IR code");
    let code = {
        let module = lpc_lpir::parse_module(ir).expect("Failed to parse IR");
        let compiled = compile_module_to_insts(&module).expect("Failed to compile");
        compiled.to_bytes().expect("Failed to convert to bytes")
    };

    match emu.run_until_ebreak() {
        Ok(_) => emu,
        Err(e) => {
            panic!("{}\n{}", format_ir_error(&emu, &e, &code, ir), e);
        }
    }
}

/// Expect IR code to run successfully and return a specific value in a register.
pub fn expect_ir_register(ir: &str, reg: Gpr, expected: i32) {
    #[cfg(test)]
    extern crate std;

    #[cfg(test)]
    use std::println;

    let mut emu = debug_ir(ir).expect("Failed to compile IR code");
    let code = {
        let module = lpc_lpir::parse_module(ir).expect("Failed to parse IR");
        let compiled = compile_module_to_insts(&module).expect("Failed to compile");
        #[cfg(test)]
        {
            println!("\n=== Generated Assembly ===");
            let bytes = compiled.to_bytes().expect("Failed to convert to bytes");
            println!("{}", disassemble_code(&bytes));
            println!("=== End Assembly ===\n");
        }
        compiled.to_bytes().expect("Failed to convert to bytes")
    };

    match emu.run_until_ebreak() {
        Ok(_) => {
            let actual = emu.get_register(reg);
            if actual != expected {
                panic!(
                    "{}",
                    format_ir_register_mismatch(&emu, &code, ir, reg, expected, actual)
                );
            }
        }
        Err(e) => {
            panic!("{}\n{}", format_ir_error(&emu, &e, &code, ir), e);
        }
    }
}

/// Expect IR code to run successfully and return a specific value in a0 (convenience function).
pub fn expect_ir_a0(ir: &str, expected: i32) {
    expect_ir_register(ir, Gpr::A0, expected);
}

/// Expect IR code to run until ECALL (syscall) and check syscall info.
pub fn expect_ir_syscall(ir: &str, expected_number: i32, expected_args: &[i32]) -> Riscv32Emulator {
    #[cfg(test)]
    extern crate std;

    #[cfg(test)]
    use std::println;

    let mut emu = debug_ir(ir).expect("Failed to compile IR code");
    let code = {
        let module = lpc_lpir::parse_module(ir).expect("Failed to parse IR");
        let compiled = compile_module_to_insts(&module).expect("Failed to compile");
        #[cfg(test)]
        {
            println!("\n=== Generated Assembly ===");
            let bytes = compiled.to_bytes().expect("Failed to convert to bytes");
            println!("{}", disassemble_code(&bytes));
            println!("=== End Assembly ===\n");
        }
        compiled.to_bytes().expect("Failed to convert to bytes")
    };

    loop {
        match emu.step() {
            Ok(StepResult::Syscall(syscall_info)) => {
                if syscall_info.number != expected_number {
                    panic!(
                        "Syscall number mismatch: expected {}, got {}\n\nIR:\n{}",
                        expected_number, syscall_info.number, ir
                    );
                }

                if expected_args.len() > 7 {
                    panic!(
                        "Too many expected args: {} (max 7)\n\nIR:\n{}",
                        expected_args.len(),
                        ir
                    );
                }

                for (i, &expected_arg) in expected_args.iter().enumerate() {
                    let actual_arg = syscall_info.args[i];
                    if actual_arg != expected_arg {
                        panic!(
                            "Syscall arg[{}] mismatch: expected {}, got {}\n\nIR:\n{}",
                            i, expected_arg, actual_arg, ir
                        );
                    }
                }

                return emu;
            }
            Ok(StepResult::Halted) => {
                panic!("Program halted before syscall\n\nIR:\n{}", ir);
            }
            Ok(StepResult::Continue) => {
                // Continue execution
            }
            Err(e) => {
                panic!("{}\n{}", format_ir_error(&emu, &e, &code, ir), e);
            }
        }
    }
}

/// Expect IR code to fail with a specific error type.
pub fn expect_ir_error<F>(ir: &str, check: F)
where
    F: FnOnce(&EmulatorError) -> bool,
{
    expect_ir_error_with_ram(ir, 1024 * 1024, check)
}

/// Expect IR code to fail with a specific error type, with custom RAM size.
pub fn expect_ir_error_with_ram<F>(ir: &str, ram_size: usize, check: F)
where
    F: FnOnce(&EmulatorError) -> bool,
{
    let mut emu = debug_ir_with_ram(ir, ram_size).expect("Failed to compile IR code");
    let code = {
        let module = lpc_lpir::parse_module(ir).expect("Failed to parse IR");
        let compiled = compile_module_to_insts(&module).expect("Failed to compile");
        compiled.to_bytes().expect("Failed to convert to bytes")
    };

    match emu.run_until_ebreak() {
        Ok(_) => {
            panic!("Expected error but execution succeeded\n\nIR:\n{}", ir);
        }
        Err(e) => {
            if !check(&e) {
                panic!(
                    "Error check failed\n{}\n{}",
                    format_ir_error(&emu, &e, &code, ir),
                    e
                );
            }
        }
    }
}

/// Expect IR code to fail with an InvalidMemoryAccess error.
pub fn expect_ir_memory_error(ir: &str) {
    expect_ir_memory_error_with_ram(ir, 1024)
}

/// Expect IR code to fail with an InvalidMemoryAccess error, with custom RAM size.
pub fn expect_ir_memory_error_with_ram(ir: &str, ram_size: usize) {
    expect_ir_error_with_ram(ir, ram_size, |e| {
        matches!(e, EmulatorError::InvalidMemoryAccess { .. })
    });
}

/// Expect IR code to fail with an UnalignedAccess error.
pub fn expect_ir_unaligned_error(ir: &str) {
    expect_ir_error(ir, |e| matches!(e, EmulatorError::UnalignedAccess { .. }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expect_ir_a0_simple() {
        expect_ir_a0(
            r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    v0 = iconst 42
    halt
}
}"#,
            42,
        );
    }

    #[test]
    fn test_expect_ir_syscall() {
        expect_ir_syscall(
            r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    v0 = iconst 42
    syscall 0(v0)
    halt
}
}"#,
            0,
            &[42],
        );
    }
}
