//! Basic tests for the RISC-V 32 emulator.

use riscv32_emulator::{LogLevel, Riscv32Emulator};
use riscv32_encoder::{add, addi, beq, ebreak, jal, lui, lw, mul, sub, sw, Gpr};

#[test]
fn test_add_instruction() {
    let mut code = Vec::new();
    // addi a0, zero, 5
    code.extend_from_slice(&addi(Gpr::A0, Gpr::ZERO, 5).to_le_bytes());
    // addi a1, zero, 10
    code.extend_from_slice(&addi(Gpr::A1, Gpr::ZERO, 10).to_le_bytes());
    // add a0, a0, a1  (store result in a0 for return)
    code.extend_from_slice(&add(Gpr::A0, Gpr::A0, Gpr::A1).to_le_bytes());
    // ebreak
    code.extend_from_slice(&ebreak().to_le_bytes());

    let mut emu = Riscv32Emulator::new(code, vec![0; 1024])
        .with_log_level(LogLevel::Instructions);

    let result = emu.run_until_ebreak().expect("Execution failed");
    assert_eq!(result, 15, "Expected 5 + 10 = 15");
    assert_eq!(emu.get_register(Gpr::A0), 15);
    assert_eq!(emu.get_register(Gpr::A1), 10);
}

#[test]
fn test_sub_instruction() {
    let mut code = Vec::new();
    // addi a0, zero, 20
    code.extend_from_slice(&addi(Gpr::A0, Gpr::ZERO, 20).to_le_bytes());
    // addi a1, zero, 7
    code.extend_from_slice(&addi(Gpr::A1, Gpr::ZERO, 7).to_le_bytes());
    // sub a0, a0, a1  (store result in a0)
    code.extend_from_slice(&sub(Gpr::A0, Gpr::A0, Gpr::A1).to_le_bytes());
    // ebreak
    code.extend_from_slice(&ebreak().to_le_bytes());

    let mut emu = Riscv32Emulator::new(code, vec![0; 1024]);
    let result = emu.run_until_ebreak().expect("Execution failed");
    assert_eq!(result, 13, "Expected 20 - 7 = 13");
    assert_eq!(emu.get_register(Gpr::A0), 13);
}

#[test]
fn test_mul_instruction() {
    let mut code = Vec::new();
    // addi a0, zero, 6
    code.extend_from_slice(&addi(Gpr::A0, Gpr::ZERO, 6).to_le_bytes());
    // addi a1, zero, 7
    code.extend_from_slice(&addi(Gpr::A1, Gpr::ZERO, 7).to_le_bytes());
    // mul a0, a0, a1  (store result in a0)
    code.extend_from_slice(&mul(Gpr::A0, Gpr::A0, Gpr::A1).to_le_bytes());
    // ebreak
    code.extend_from_slice(&ebreak().to_le_bytes());

    let mut emu = Riscv32Emulator::new(code, vec![0; 1024]);
    let result = emu.run_until_ebreak().expect("Execution failed");
    assert_eq!(result, 42, "Expected 6 * 7 = 42");
    assert_eq!(emu.get_register(Gpr::A0), 42);
}

#[test]
fn test_load_store() {
    let mut code = Vec::new();
    // lui sp, 0x80000  (set sp to 0x80000000)
    code.extend_from_slice(&lui(Gpr::SP, 0x80000).to_le_bytes());
    // addi sp, sp, 0x100  (sp = 0x80000100)
    code.extend_from_slice(&addi(Gpr::SP, Gpr::SP, 0x100).to_le_bytes());
    // addi a0, zero, 42
    code.extend_from_slice(&addi(Gpr::A0, Gpr::ZERO, 42).to_le_bytes());
    // sw a0, 0(sp)
    code.extend_from_slice(&sw(Gpr::SP, Gpr::A0, 0).to_le_bytes());
    // lw a0, 0(sp)  (load back into a0)
    code.extend_from_slice(&lw(Gpr::A0, Gpr::SP, 0).to_le_bytes());
    // ebreak
    code.extend_from_slice(&ebreak().to_le_bytes());

    let mut emu = Riscv32Emulator::new(code, vec![0; 1024 * 1024]);
    let result = emu.run_until_ebreak().expect("Execution failed");
    assert_eq!(result, 42);
    assert_eq!(emu.get_register(Gpr::A0), 42);
}

#[test]
fn test_jal() {
    let mut code = Vec::new();
    // jal ra, 8  (skip next instruction)
    code.extend_from_slice(&jal(Gpr::RA, 8).to_le_bytes());
    // addi a0, zero, 1  (should be skipped)
    code.extend_from_slice(&addi(Gpr::A0, Gpr::ZERO, 1).to_le_bytes());
    // addi a0, zero, 42  (target of jump)
    code.extend_from_slice(&addi(Gpr::A0, Gpr::ZERO, 42).to_le_bytes());
    // ebreak
    code.extend_from_slice(&ebreak().to_le_bytes());

    let mut emu = Riscv32Emulator::new(code, vec![0; 1024]);
    let result = emu.run_until_ebreak().expect("Execution failed");
    assert_eq!(result, 42);
    assert_eq!(emu.get_register(Gpr::RA), 4); // PC + 4 of jal instruction
}

#[test]
fn test_branch_beq() {
    let mut code = Vec::new();
    // addi a0, zero, 5
    code.extend_from_slice(&addi(Gpr::A0, Gpr::ZERO, 5).to_le_bytes());
    // addi a1, zero, 5
    code.extend_from_slice(&addi(Gpr::A1, Gpr::ZERO, 5).to_le_bytes());
    // beq a0, a1, 8  (branch if equal)
    code.extend_from_slice(&beq(Gpr::A0, Gpr::A1, 8).to_le_bytes());
    // addi a0, zero, 1  (should be skipped)
    code.extend_from_slice(&addi(Gpr::A0, Gpr::ZERO, 1).to_le_bytes());
    // addi a0, zero, 42  (target of branch)
    code.extend_from_slice(&addi(Gpr::A0, Gpr::ZERO, 42).to_le_bytes());
    // ebreak
    code.extend_from_slice(&ebreak().to_le_bytes());

    let mut emu = Riscv32Emulator::new(code, vec![0; 1024]);
    let result = emu.run_until_ebreak().expect("Execution failed");
    assert_eq!(result, 42);
}

#[test]
fn test_instruction_limit() {
    let mut code = Vec::new();
    // Infinite loop: addi a0, a0, 1; jal zero, -4
    code.extend_from_slice(&addi(Gpr::A0, Gpr::A0, 1).to_le_bytes());
    code.extend_from_slice(&jal(Gpr::ZERO, -4).to_le_bytes());

    let mut emu = Riscv32Emulator::new(code, vec![0; 1024])
        .with_max_instructions(100);

    let result = emu.run_until_ebreak();
    assert!(result.is_err(), "Expected instruction limit error");
    match result {
        Err(riscv32_emulator::EmulatorError::InstructionLimitExceeded { limit, executed, .. }) => {
            assert_eq!(limit, 100);
            assert_eq!(executed, 100);
        }
        _ => panic!("Expected InstructionLimitExceeded error"),
    }
}

#[test]
fn test_zero_register() {
    let mut code = Vec::new();
    // addi a0, zero, 100
    code.extend_from_slice(&addi(Gpr::A0, Gpr::ZERO, 100).to_le_bytes());
    // add zero, a0, a0  (write to zero - should be no-op)
    code.extend_from_slice(&add(Gpr::ZERO, Gpr::A0, Gpr::A0).to_le_bytes());
    // ebreak
    code.extend_from_slice(&ebreak().to_le_bytes());

    let mut emu = Riscv32Emulator::new(code, vec![0; 1024]);
    let result = emu.run_until_ebreak().expect("Execution failed");
    assert_eq!(result, 100);
    assert_eq!(emu.get_register(Gpr::ZERO), 0);
}

#[test]
fn test_lui() {
    let mut code = Vec::new();
    // lui a0, 0x12345  (a0 = 0x12345000)
    code.extend_from_slice(&lui(Gpr::A0, 0x12345).to_le_bytes());
    // ebreak
    code.extend_from_slice(&ebreak().to_le_bytes());

    let mut emu = Riscv32Emulator::new(code, vec![0; 1024]);
    let result = emu.run_until_ebreak().expect("Execution failed");
    assert_eq!(result, 0x12345000);
}

