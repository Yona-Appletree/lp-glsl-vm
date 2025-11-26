//! Error types for the RISC-V 32 emulator.

extern crate alloc;

use alloc::string::String;
use riscv32_encoder::Gpr;

/// Kind of memory access that failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryAccessKind {
    Read,
    Write,
    InstructionFetch,
}

/// Errors that can occur during emulation.
#[derive(Debug, Clone)]
pub enum EmulatorError {
    /// Instruction limit exceeded.
    InstructionLimitExceeded {
        limit: u64,
        executed: u64,
        pc: u32,
        regs: [i32; 32],
    },
    /// Invalid memory access (out of bounds).
    InvalidMemoryAccess {
        address: u32,
        size: usize,
        kind: MemoryAccessKind,
        pc: u32,
        regs: [i32; 32],
    },
    /// Invalid instruction encoding.
    InvalidInstruction {
        pc: u32,
        instruction: u32,
        reason: String,
        regs: [i32; 32],
    },
    /// Unaligned memory access.
    UnalignedAccess {
        address: u32,
        alignment: usize,
        pc: u32,
        regs: [i32; 32],
    },
    /// Unknown or unsupported opcode.
    UnknownOpcode {
        opcode: u8,
        pc: u32,
        instruction: u32,
        regs: [i32; 32],
    },
    /// Invalid register access.
    InvalidRegister {
        reg: Gpr,
        pc: u32,
        reason: String,
    },
}

impl EmulatorError {
    /// Get the PC where the error occurred.
    pub fn pc(&self) -> u32 {
        match self {
            EmulatorError::InstructionLimitExceeded { pc, .. } => *pc,
            EmulatorError::InvalidMemoryAccess { pc, .. } => *pc,
            EmulatorError::InvalidInstruction { pc, .. } => *pc,
            EmulatorError::UnalignedAccess { pc, .. } => *pc,
            EmulatorError::UnknownOpcode { pc, .. } => *pc,
            EmulatorError::InvalidRegister { pc, .. } => *pc,
        }
    }

    /// Get a snapshot of register state at the time of error.
    pub fn regs(&self) -> Option<&[i32; 32]> {
        match self {
            EmulatorError::InstructionLimitExceeded { regs, .. } => Some(regs),
            EmulatorError::InvalidMemoryAccess { regs, .. } => Some(regs),
            EmulatorError::InvalidInstruction { regs, .. } => Some(regs),
            EmulatorError::UnalignedAccess { regs, .. } => Some(regs),
            EmulatorError::UnknownOpcode { regs, .. } => Some(regs),
            EmulatorError::InvalidRegister { .. } => None,
        }
    }
}

impl core::fmt::Display for EmulatorError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            EmulatorError::InstructionLimitExceeded {
                limit,
                executed,
                pc,
                ..
            } => write!(
                f,
                "Instruction limit exceeded: executed {} instructions (limit: {}) at PC 0x{:08x}",
                executed, limit, pc
            ),
            EmulatorError::InvalidMemoryAccess {
                address,
                size,
                kind,
                pc,
                ..
            } => {
                let kind_str = match kind {
                    MemoryAccessKind::Read => "read",
                    MemoryAccessKind::Write => "write",
                    MemoryAccessKind::InstructionFetch => "instruction fetch",
                };
                write!(
                    f,
                    "Invalid memory {} at address 0x{:08x} (size: {} bytes) at PC 0x{:08x}",
                    kind_str, address, size, pc
                )
            }
            EmulatorError::InvalidInstruction {
                pc,
                instruction,
                reason,
                ..
            } => write!(
                f,
                "Invalid instruction 0x{:08x} at PC 0x{:08x}: {}",
                instruction, pc, reason
            ),
            EmulatorError::UnalignedAccess {
                address,
                alignment,
                pc,
                ..
            } => write!(
                f,
                "Unaligned memory access at address 0x{:08x} (requires {} byte alignment) at PC 0x{:08x}",
                address, alignment, pc
            ),
            EmulatorError::UnknownOpcode {
                opcode,
                pc,
                instruction,
                ..
            } => write!(
                f,
                "Unknown opcode 0x{:02x} in instruction 0x{:08x} at PC 0x{:08x}",
                opcode, instruction, pc
            ),
            EmulatorError::InvalidRegister { reg, pc, reason } => write!(
                f,
                "Invalid register access: {:?} at PC 0x{:08x}: {}",
                reg, pc, reason
            ),
        }
    }
}

// Note: std::error::Error trait implementation would require std feature

