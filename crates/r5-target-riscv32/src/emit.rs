//! Code emission for RISC-V 32-bit.

use alloc::vec::Vec;

/// A code buffer that accumulates RISC-V 32-bit instructions.
pub struct CodeBuffer {
    instructions: Vec<u32>,
}

impl CodeBuffer {
    /// Create a new empty code buffer.
    pub fn new() -> Self {
        Self {
            instructions: Vec::new(),
        }
    }

    /// Emit a 32-bit instruction.
    pub fn emit(&mut self, inst: u32) {
        self.instructions.push(inst);
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
    pub fn as_bytes(&self) -> &[u8] {
        unsafe {
            core::slice::from_raw_parts(
                self.instructions.as_ptr() as *const u8,
                self.instructions.len() * 4,
            )
        }
    }

    /// Get the code as a u32 slice.
    pub fn as_instructions(&self) -> &[u32] {
        &self.instructions
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
    use riscv32_encoder::addi;

    use super::*;

    #[test]
    fn test_code_buffer() {
        let mut buf = CodeBuffer::new();
        assert_eq!(buf.len(), 0);
        assert_eq!(buf.instruction_count(), 0);

        // Emit an instruction
        let inst = addi(riscv32_encoder::Gpr::A0, riscv32_encoder::Gpr::A1, 5);
        buf.emit(inst);
        assert_eq!(buf.len(), 4);
        assert_eq!(buf.instruction_count(), 1);

        // Emit another instruction
        buf.emit(inst);
        assert_eq!(buf.len(), 8);
        assert_eq!(buf.instruction_count(), 2);

        // Check bytes
        let bytes = buf.as_bytes();
        assert_eq!(bytes.len(), 8);
        assert_eq!(bytes[0..4], inst.to_le_bytes());
    }
}
