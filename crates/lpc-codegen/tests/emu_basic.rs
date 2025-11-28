//! Basic tests for the RISC-V 32 emu.

use lpc_codegen::{assemble_code, Gpr, LogLevel, Riscv32Emulator};

#[test]
fn test_add_instruction() {
    let code = assemble_code(
        "addi a0, zero, 5
addi a1, zero, 10
add a0, a0, a1
ebreak",
        None,
    )
    .unwrap();

    let mut emu = Riscv32Emulator::new(code, vec![0; 1024]).with_log_level(LogLevel::Instructions);

    let result = emu.run_until_ebreak().expect("Execution failed");
    assert_eq!(result, 15, "Expected 5 + 10 = 15");
    assert_eq!(emu.get_register(Gpr::A0), 15);
    assert_eq!(emu.get_register(Gpr::A1), 10);
}

#[test]
fn test_sub_instruction() {
    let code = assemble_code(
        "addi a0, zero, 20
addi a1, zero, 7
sub a0, a0, a1
ebreak",
        None,
    )
    .unwrap();

    let mut emu = Riscv32Emulator::new(code, vec![0; 1024]);
    let result = emu.run_until_ebreak().expect("Execution failed");
    assert_eq!(result, 13, "Expected 20 - 7 = 13");
    assert_eq!(emu.get_register(Gpr::A0), 13);
}

#[test]
fn test_mul_instruction() {
    let code = assemble_code(
        "addi a0, zero, 6
addi a1, zero, 7
mul a0, a0, a1
ebreak",
        None,
    )
    .unwrap();

    let mut emu = Riscv32Emulator::new(code, vec![0; 1024]);
    let result = emu.run_until_ebreak().expect("Execution failed");
    assert_eq!(result, 42, "Expected 6 * 7 = 42");
    assert_eq!(emu.get_register(Gpr::A0), 42);
}

#[test]
fn test_load_store() {
    let code = assemble_code(
        "lui sp, 0x80000000
addi sp, sp, 0x100
addi a0, zero, 42
sw a0, 0(sp)
lw a0, 0(sp)
ebreak",
        None,
    )
    .unwrap();

    let mut emu = Riscv32Emulator::new(code, vec![0; 1024 * 1024]);
    let result = emu.run_until_ebreak().expect("Execution failed");
    assert_eq!(result, 42);
    assert_eq!(emu.get_register(Gpr::A0), 42);
}

#[test]
fn test_jal() {
    let code = assemble_code(
        "jal ra, 8
addi a0, zero, 1
addi a0, zero, 42
ebreak",
        None,
    )
    .unwrap();

    let mut emu = Riscv32Emulator::new(code, vec![0; 1024]);
    let result = emu.run_until_ebreak().expect("Execution failed");
    assert_eq!(result, 42);
    assert_eq!(emu.get_register(Gpr::Ra), 4); // PC + 4 of jal instruction
}

#[test]
fn test_branch_beq() {
    let code = assemble_code(
        "addi a0, zero, 5
addi a1, zero, 5
beq a0, a1, 8
addi a0, zero, 1
addi a0, zero, 42
ebreak",
        None,
    )
    .unwrap();

    let mut emu = Riscv32Emulator::new(code, vec![0; 1024]);
    let result = emu.run_until_ebreak().expect("Execution failed");
    assert_eq!(result, 42);
}

#[test]
fn test_instruction_limit() {
    let code = assemble_code(
        "addi a0, a0, 1
jal zero, -4",
        None,
    )
    .unwrap();

    let mut emu = Riscv32Emulator::new(code, vec![0; 1024]).with_max_instructions(100);

    let result = emu.run_until_ebreak();
    assert!(result.is_err(), "Expected instruction limit error");
    match result {
        Err(lpc_codegen::EmulatorError::InstructionLimitExceeded {
            limit, executed, ..
        }) => {
            assert_eq!(limit, 100);
            assert_eq!(executed, 100);
        }
        _ => panic!("Expected InstructionLimitExceeded error"),
    }
}

#[test]
fn test_zero_register() {
    let code = assemble_code(
        "addi a0, zero, 100
add zero, a0, a0
ebreak",
        None,
    )
    .unwrap();

    let mut emu = Riscv32Emulator::new(code, vec![0; 1024]);
    let result = emu.run_until_ebreak().expect("Execution failed");
    assert_eq!(result, 100);
    assert_eq!(emu.get_register(Gpr::Zero), 0);
}

#[test]
fn test_lui() {
    let code = assemble_code(
        "lui a0, 0x12345000
ebreak",
        None,
    )
    .unwrap();

    let mut emu = Riscv32Emulator::new(code, vec![0; 1024]);
    let result = emu.run_until_ebreak().expect("Execution failed");
    assert_eq!(result, 0x12345000);
}
