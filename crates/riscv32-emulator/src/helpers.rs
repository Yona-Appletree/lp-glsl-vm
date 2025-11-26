//! Helper functions for testing RISC-V code.

extern crate alloc;

use alloc::{format, string::String, vec, vec::Vec};

use riscv32_encoder::{assemble_code, disassemble_instruction, Gpr, Inst};

use crate::{emulator::Riscv32Emulator, error::EmulatorError, logging::LogLevel};

/// Create an emulator from assembly code.
pub fn debug_riscv32_asm(asm: &str) -> Result<Riscv32Emulator, EmulatorError> {
    debug_riscv32_asm_with_ram(asm, 1024 * 1024)
}

/// Create an emulator from assembly code with specified RAM size.
pub fn debug_riscv32_asm_with_ram(
    asm: &str,
    ram_size: usize,
) -> Result<Riscv32Emulator, EmulatorError> {
    let code = assemble_code(asm, None).map_err(|e| EmulatorError::InvalidInstruction {
        pc: 0,
        instruction: 0,
        reason: format!("Assembly error: {}", e),
        regs: [0; 32],
    })?;
    Ok(Riscv32Emulator::new(code, vec![0; ram_size]).with_log_level(LogLevel::Instructions))
}

/// Create an emulator from binary code bytes.
pub fn debug_riscv32_bytes(bytes: &[u8]) -> Result<Riscv32Emulator, EmulatorError> {
    Ok(Riscv32Emulator::new(bytes.to_vec(), vec![0; 1024 * 1024])
        .with_log_level(LogLevel::Instructions))
}

/// Create an emulator from instruction structs.
pub fn debug_riscv32_ops(ops: &[Inst]) -> Result<Riscv32Emulator, EmulatorError> {
    let mut code = Vec::new();
    for op in ops {
        code.extend_from_slice(&op.encode().to_le_bytes());
    }
    Ok(Riscv32Emulator::new(code, vec![0; 1024 * 1024]).with_log_level(LogLevel::Instructions))
}

/// Format error with disassembly and logs.
fn format_error(emu: &Riscv32Emulator, error: &EmulatorError, code: &[u8]) -> String {
    let mut result = String::new();

    // Get error PC
    let error_pc = error.pc();

    // Disassemble all instructions
    let mut instructions = Vec::new();
    for i in (0..code.len()).step_by(4) {
        if i + 4 <= code.len() {
            let inst_word = u32::from_le_bytes([code[i], code[i + 1], code[i + 2], code[i + 3]]);
            let pc = i as u32;
            let disasm = disassemble_instruction(inst_word);
            instructions.push((pc, inst_word, disasm));
        }
    }

    result.push_str("=== RISC-V Execution Error ===\n\n");
    result.push_str(&format!("Error: {}\n", error));
    result.push_str(&format!("PC: 0x{:08x}\n\n", error_pc));

    // Show disassembly
    result.push_str("Disassembly:\n");
    if instructions.len() > 50 {
        // Find the index of the failing instruction
        let fail_idx = instructions
            .iter()
            .position(|(pc, _, _)| *pc == error_pc)
            .unwrap_or(0);
        let start = fail_idx.saturating_sub(10);
        let end = (fail_idx + 11).min(instructions.len());

        if start > 0 {
            result.push_str("  ...\n");
        }

        for (idx, (pc, _inst_word, disasm)) in instructions[start..end].iter().enumerate() {
            let actual_idx = start + idx;
            let marker = if *pc == error_pc { ">>> " } else { "    " };
            result.push_str(&format!(
                "{}{:3}: 0x{:08x}: {}\n",
                marker, actual_idx, pc, disasm
            ));
        }

        if end < instructions.len() {
            result.push_str("  ...\n");
        }
    } else {
        // Show all instructions
        for (idx, (pc, _inst_word, disasm)) in instructions.iter().enumerate() {
            let marker = if *pc == error_pc { ">>> " } else { "    " };
            result.push_str(&format!("{}{:3}: 0x{:08x}: {}\n", marker, idx, pc, disasm));
        }
    }

    // Show last 10 logs
    let logs = emu.get_logs();
    if !logs.is_empty() {
        result.push_str("\nLast execution logs:\n");
        let start = logs.len().saturating_sub(10);
        for log in &logs[start..] {
            result.push_str(log);
        }
    }

    result
}

/// Expect code to run successfully until EBREAK, returning the emulator.
pub fn expect_ok(asm: &str) -> Riscv32Emulator {
    let mut emu = debug_riscv32_asm(asm).expect("Failed to assemble code");
    match emu.run_until_ebreak() {
        Ok(_) => emu,
        Err(e) => {
            let code = assemble_code(asm, None).unwrap();
            panic!("{}\n{}", format_error(&emu, &e, &code), e);
        }
    }
}

/// Expect code to run successfully and return a specific value in a0.
pub fn expect_register(asm: &str, reg: Gpr, expected: i32) {
    let mut emu = debug_riscv32_asm(asm).expect("Failed to assemble code");
    match emu.run_until_ebreak() {
        Ok(_) => {
            let actual = emu.get_register(reg);
            if actual != expected {
                panic!(
                    "Register {:?} mismatch: expected {}, got {}\n\nCode:\n{}",
                    reg, expected, actual, asm
                );
            }
        }
        Err(e) => {
            let code = assemble_code(asm, None).unwrap();
            panic!("{}\n{}", format_error(&emu, &e, &code), e);
        }
    }
}

/// Expect code to run successfully and return a specific value in a0 (convenience function).
pub fn expect_a0(asm: &str, expected: i32) {
    expect_register(asm, Gpr::A0, expected);
}

/// Expect code to fail with a specific error type.
pub fn expect_error<F>(asm: &str, check: F)
where
    F: FnOnce(&EmulatorError) -> bool,
{
    expect_error_with_ram(asm, 1024 * 1024, check)
}

/// Expect code to fail with a specific error type, with custom RAM size.
pub fn expect_error_with_ram<F>(asm: &str, ram_size: usize, check: F)
where
    F: FnOnce(&EmulatorError) -> bool,
{
    let mut emu = debug_riscv32_asm_with_ram(asm, ram_size).expect("Failed to assemble code");
    match emu.run_until_ebreak() {
        Ok(_) => {
            panic!("Expected error but execution succeeded\n\nCode:\n{}", asm);
        }
        Err(e) => {
            if !check(&e) {
                let code = assemble_code(asm, None).unwrap();
                panic!(
                    "Error check failed\n{}\n{}",
                    format_error(&emu, &e, &code),
                    e
                );
            }
        }
    }
}

/// Expect code to fail with an InvalidMemoryAccess error.
pub fn expect_memory_error(asm: &str) {
    expect_memory_error_with_ram(asm, 1024)
}

/// Expect code to fail with an InvalidMemoryAccess error, with custom RAM size.
pub fn expect_memory_error_with_ram(asm: &str, ram_size: usize) {
    expect_error_with_ram(asm, ram_size, |e| {
        matches!(e, EmulatorError::InvalidMemoryAccess { .. })
    });
}

/// Expect code to fail with an UnalignedAccess error.
pub fn expect_unaligned_error(asm: &str) {
    expect_error(asm, |e| matches!(e, EmulatorError::UnalignedAccess { .. }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expect_a0_simple() {
        expect_a0(
            "
addi a0, zero, 42
ebreak",
            42,
        );
    }

    #[test]
    fn test_expect_a0_arithmetic() {
        expect_a0(
            "
addi a0, zero, 5
addi a1, zero, 10
add a0, a0, a1
ebreak",
            15,
        );
    }

    #[test]
    fn test_expect_a0_memory() {
        expect_a0(
            "
lui sp, 0x80000000
addi sp, sp, 0x100
addi a0, zero, 42
sw a0, 0(sp)
lw a0, 0(sp)
ebreak",
            42,
        );
    }

    #[test]
    fn test_expect_register() {
        expect_register(
            "
addi a1, zero, 100
ebreak",
            Gpr::A1,
            100,
        );
    }

    #[test]
    fn test_expect_memory_error() {
        expect_memory_error_with_ram(
            "
lui sp, 0x80000000
lw a0, 0x400(sp)
ebreak
            ",
            1024, // Small RAM - 0x400 is out of bounds
        );
    }

    #[test]
    fn test_expect_unaligned_error() {
        expect_unaligned_error(
            "
lui sp, 0x80000000
lw a0, 1(sp)
ebreak",
        );
    }

    #[test]
    fn test_expect_ok() {
        let emu = expect_ok(
            "
addi a0, zero, 42
ebreak
        ");
        assert_eq!(emu.get_register(Gpr::A0), 42);
    }
}
