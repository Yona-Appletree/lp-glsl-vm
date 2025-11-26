//! Code emission for RISC-V 32-bit.

use alloc::vec::Vec;

use riscv32_encoder::Inst;

/// A code buffer that accumulates RISC-V 32-bit instructions.
///
/// Instructions are stored in structured form and encoded to binary
/// only when `as_bytes()` is called (lazy encoding, like Cranelift).
pub struct CodeBuffer {
    instructions: Vec<Inst>,
}

impl CodeBuffer {
    /// Create a new empty code buffer.
    pub fn new() -> Self {
        Self {
            instructions: Vec::new(),
        }
    }

    /// Emit a structured instruction.
    pub fn emit(&mut self, inst: Inst) {
        self.instructions.push(inst);
    }

    /// Get the structured instructions.
    pub fn instructions(&self) -> &[Inst] {
        &self.instructions
    }

    /// Get the current code size in bytes.
    pub fn len(&self) -> usize {
        self.instructions.len() * 4
    }

    /// Get the current instruction count.
    pub fn instruction_count(&self) -> usize {
        self.instructions.len()
    }

    /// Get the code as a byte slice.
    ///
    /// This encodes instructions lazily on-demand.
    pub fn as_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.instructions.len() * 4);
        for inst in &self.instructions {
            let encoded = inst.encode();
            bytes.extend_from_slice(&encoded.to_le_bytes());
        }
        bytes
    }

    /// Get the code as a u32 slice (encoded instructions).
    ///
    /// This encodes instructions lazily on-demand.
    pub fn as_instructions(&self) -> Vec<u32> {
        self.instructions.iter().map(|inst| inst.encode()).collect()
    }

    /// Clear the buffer.
    pub fn clear(&mut self) {
        self.instructions.clear();
    }
}

impl Default for CodeBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use riscv32_encoder::{Gpr, Inst};

    use super::*;

    #[test]
    fn test_code_buffer() {
        let mut buf = CodeBuffer::new();
        assert_eq!(buf.len(), 0);
        assert_eq!(buf.instruction_count(), 0);

        // Emit an instruction
        let inst = Inst::Addi {
            rd: Gpr::A0,
            rs1: Gpr::A1,
            imm: 5,
        };
        buf.emit(inst.clone());
        assert_eq!(buf.len(), 4);
        assert_eq!(buf.instruction_count(), 1);

        // Emit another instruction
        buf.emit(inst.clone());
        assert_eq!(buf.len(), 8);
        assert_eq!(buf.instruction_count(), 2);

        // Check bytes
        let bytes = buf.as_bytes();
        assert_eq!(bytes.len(), 8);
        let expected_bytes = inst.encode().to_le_bytes();
        assert_eq!(bytes[0..4], expected_bytes);
    }
}
