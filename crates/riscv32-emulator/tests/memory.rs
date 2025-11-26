//! Memory access tests for the RISC-V 32 emulator.

use riscv32_emulator::Riscv32Emulator;
use riscv32_encoder::{addi, lui, lw, sw, Gpr};

#[test]
fn test_unaligned_access() {
    let mut code = Vec::new();
    // lui sp, 0x80000
    code.extend_from_slice(&lui(Gpr::SP, 0x80000).to_le_bytes());
    // addi sp, sp, 1  (unaligned address)
    code.extend_from_slice(&addi(Gpr::SP, Gpr::SP, 1).to_le_bytes());
    // lw a0, 0(sp)  (should fail - unaligned)
    code.extend_from_slice(&lw(Gpr::A0, Gpr::SP, 0).to_le_bytes());

    let mut emu = Riscv32Emulator::new(code, vec![0; 1024]);
    let result = emu.step(); // lui
    assert!(result.is_ok());
    let result = emu.step(); // addi
    assert!(result.is_ok());
    let result = emu.step(); // lw - should fail
    assert!(result.is_err());
    match result {
        Err(riscv32_emulator::EmulatorError::UnalignedAccess { address, alignment, .. }) => {
            assert_eq!(address, 0x80000001);
            assert_eq!(alignment, 4);
        }
        _ => panic!("Expected UnalignedAccess error"),
    }
}

#[test]
fn test_out_of_bounds_read() {
    let mut code = Vec::new();
    // lui sp, 0x80000
    code.extend_from_slice(&lui(Gpr::SP, 0x80000).to_le_bytes());
    // addi sp, sp, 0x1000000  (out of bounds)
    code.extend_from_slice(&addi(Gpr::SP, Gpr::SP, 0x1000000).to_le_bytes());
    // lw a0, 0(sp)  (should fail - out of bounds)
    code.extend_from_slice(&lw(Gpr::A0, Gpr::SP, 0).to_le_bytes());

    let mut emu = Riscv32Emulator::new(code, vec![0; 1024]);
    let result = emu.step(); // lui
    assert!(result.is_ok());
    let result = emu.step(); // addi
    assert!(result.is_ok());
    let result = emu.step(); // lw - should fail
    assert!(result.is_err());
    match result {
        Err(riscv32_emulator::EmulatorError::InvalidMemoryAccess { address, kind, .. }) => {
            assert_eq!(address, 0x80000000 + 0x1000000);
            assert_eq!(kind, riscv32_emulator::MemoryAccessKind::Read);
        }
        _ => panic!("Expected InvalidMemoryAccess error"),
    }
}

#[test]
fn test_out_of_bounds_write() {
    let mut code = Vec::new();
    // lui sp, 0x80000
    code.extend_from_slice(&lui(Gpr::SP, 0x80000).to_le_bytes());
    // addi sp, sp, 0x1000000  (out of bounds)
    code.extend_from_slice(&addi(Gpr::SP, Gpr::SP, 0x1000000).to_le_bytes());
    // addi a0, zero, 42
    code.extend_from_slice(&addi(Gpr::A0, Gpr::ZERO, 42).to_le_bytes());
    // sw a0, 0(sp)  (should fail - out of bounds)
    code.extend_from_slice(&sw(Gpr::SP, Gpr::A0, 0).to_le_bytes());

    let mut emu = Riscv32Emulator::new(code, vec![0; 1024]);
    let result = emu.step(); // lui
    assert!(result.is_ok());
    let result = emu.step(); // addi
    assert!(result.is_ok());
    let result = emu.step(); // addi
    assert!(result.is_ok());
    let result = emu.step(); // sw - should fail
    assert!(result.is_err());
    match result {
        Err(riscv32_emulator::EmulatorError::InvalidMemoryAccess { address, kind, .. }) => {
            assert_eq!(address, 0x80000000 + 0x1000000);
            assert_eq!(kind, riscv32_emulator::MemoryAccessKind::Write);
        }
        _ => panic!("Expected InvalidMemoryAccess error"),
    }
}

#[test]
fn test_write_to_code_region() {
    let mut code = Vec::new();
    // addi a0, zero, 42
    code.extend_from_slice(&addi(Gpr::A0, Gpr::ZERO, 42).to_le_bytes());
    // sw a0, 0(zero)  (try to write to code region - should fail)
    code.extend_from_slice(&sw(Gpr::ZERO, Gpr::A0, 0).to_le_bytes());

    let mut emu = Riscv32Emulator::new(code, vec![0; 1024]);
    let result = emu.step(); // addi
    assert!(result.is_ok());
    let result = emu.step(); // sw - should fail
    assert!(result.is_err());
    match result {
        Err(riscv32_emulator::EmulatorError::InvalidMemoryAccess { address, kind, .. }) => {
            assert_eq!(address, 0);
            assert_eq!(kind, riscv32_emulator::MemoryAccessKind::Write);
        }
        _ => panic!("Expected InvalidMemoryAccess error"),
    }
}

