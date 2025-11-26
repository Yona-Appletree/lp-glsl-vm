//! Logging infrastructure for the RISC-V 32 emulator.

extern crate alloc;

use alloc::{format, string::String, vec::Vec};
use riscv32_encoder::Gpr;

/// Logging verbosity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    /// No logging.
    None,
    /// Only log errors.
    Errors,
    /// Log each instruction execution.
    Instructions,
    /// Full verbose logging with register and memory state.
    Verbose,
}

/// Log entry for a single instruction execution.
#[derive(Debug, Clone)]
pub struct InstructionLog {
    pub pc: u32,
    pub instruction: u32,
    pub disassembly: String,
    pub regs_read: Vec<(Gpr, i32)>,
    pub regs_written: Vec<(Gpr, i32, i32)>, // (reg, old_value, new_value)
    pub memory_reads: Vec<(u32, i32)>,      // (address, value)
    pub memory_writes: Vec<(u32, i32, i32)>, // (address, old_value, new_value)
    pub pc_change: Option<(u32, u32)>,       // (old_pc, new_pc)
}

impl InstructionLog {
    /// Create a new empty instruction log.
    pub fn new(pc: u32, instruction: u32, disassembly: String) -> Self {
        Self {
            pc,
            instruction,
            disassembly,
            regs_read: Vec::new(),
            regs_written: Vec::new(),
            memory_reads: Vec::new(),
            memory_writes: Vec::new(),
            pc_change: None,
        }
    }

    /// Format the log entry as a string.
    pub fn format(&self, verbose: bool) -> String {
        let mut result = String::new();
        result.push_str(&format!("0x{:08x}: {}\n", self.pc, self.disassembly));

        if verbose {
            if !self.regs_read.is_empty() {
                result.push_str("  Reads: ");
                for (reg, value) in &self.regs_read {
                    result.push_str(&format!("{:?}={} ", reg, value));
                }
                result.push('\n');
            }

            if !self.regs_written.is_empty() {
                result.push_str("  Writes: ");
                for (reg, old_val, new_val) in &self.regs_written {
                    result.push_str(&format!("{:?}:{}->{} ", reg, old_val, new_val));
                }
                result.push('\n');
            }

            if !self.memory_reads.is_empty() {
                result.push_str("  Memory reads: ");
                for (addr, value) in &self.memory_reads {
                    result.push_str(&format!("0x{:08x}={} ", addr, value));
                }
                result.push('\n');
            }

            if !self.memory_writes.is_empty() {
                result.push_str("  Memory writes: ");
                for (addr, old_val, new_val) in &self.memory_writes {
                    result.push_str(&format!("0x{:08x}:{}->{} ", addr, old_val, new_val));
                }
                result.push('\n');
            }

            if let Some((old_pc, new_pc)) = self.pc_change {
                result.push_str(&format!("  PC: 0x{:08x} -> 0x{:08x}\n", old_pc, new_pc));
            }
        }

        result
    }
}

