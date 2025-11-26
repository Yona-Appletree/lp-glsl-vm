//! Memory access tests for the RISC-V 32 emu.

use lpc_riscv32::{assemble_code, Riscv32Emulator};

#[test]
fn test_unaligned_access() {
    let code = assemble_code(
        "lui sp, 0x80000000
lw a0, 1(sp)",
        None,
    )
    .unwrap();

    let mut emu = Riscv32Emulator::new(code, vec![0; 1024]);
    let result = emu.step(); // lui
    assert!(result.is_ok());
    let result = emu.step(); // lw - should fail
    assert!(result.is_err());
    match result {
        Err(lpc_riscv32::EmulatorError::UnalignedAccess {
            address, alignment, ..
        }) => {
            assert_eq!(address, 0x80000001);
            assert_eq!(alignment, 4);
        }
        _ => panic!("Expected UnalignedAccess error"),
    }
}

#[test]
fn test_out_of_bounds_read() {
    let code = assemble_code(
        "lui sp, 0x80000000
lw a0, 0x400(sp)",
        None,
    )
    .unwrap();

    let mut emu = Riscv32Emulator::new(code, vec![0; 1024]);
    let result = emu.step(); // lui
    assert!(result.is_ok());
    let result = emu.step(); // lw - should fail
    assert!(result.is_err());
    match result {
        Err(lpc_riscv32::EmulatorError::InvalidMemoryAccess { address, kind, .. }) => {
            assert_eq!(address, 0x80000000 + 0x400);
            assert_eq!(kind, lpc_riscv32::MemoryAccessKind::Read);
        }
        _ => panic!("Expected InvalidMemoryAccess error"),
    }
}

#[test]
fn test_out_of_bounds_write() {
    let code = assemble_code(
        "lui sp, 0x80000000
addi a0, zero, 42
sw a0, 0x400(sp)",
        None,
    )
    .unwrap();

    let mut emu = Riscv32Emulator::new(code, vec![0; 1024]);
    let result = emu.step(); // lui
    assert!(result.is_ok());
    let result = emu.step(); // addi
    assert!(result.is_ok());
    let result = emu.step(); // sw - should fail
    assert!(result.is_err());
    match result {
        Err(lpc_riscv32::EmulatorError::InvalidMemoryAccess { address, kind, .. }) => {
            assert_eq!(address, 0x80000000 + 0x400);
            assert_eq!(kind, lpc_riscv32::MemoryAccessKind::Write);
        }
        _ => panic!("Expected InvalidMemoryAccess error"),
    }
}

#[test]
fn test_write_to_code_region() {
    let code = assemble_code(
        "addi a0, zero, 42
sw a0, 0(zero)",
        None,
    )
    .unwrap();

    let mut emu = Riscv32Emulator::new(code, vec![0; 1024]);
    let result = emu.step(); // addi
    assert!(result.is_ok());
    let result = emu.step(); // sw - should fail
    assert!(result.is_err());
    match result {
        Err(lpc_riscv32::EmulatorError::InvalidMemoryAccess { address, kind, .. }) => {
            assert_eq!(address, 0);
            assert_eq!(kind, lpc_riscv32::MemoryAccessKind::Write);
        }
        _ => panic!("Expected InvalidMemoryAccess error"),
    }
}
