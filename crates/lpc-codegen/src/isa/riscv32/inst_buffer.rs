//! Code emission for RISC-V 32-bit.

use alloc::vec::Vec;

use crate::{Gpr, Inst};

/// A code buffer that accumulates RISC-V 32-bit instructions.
///
/// Instructions are stored in structured form and encoded to binary
/// only when `as_bytes()` is called (lazy encoding, like Cranelift).
pub struct InstBuffer {
    instructions: Vec<Inst>,
}

impl InstBuffer {
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

    // Convenience methods for common instructions used in prologue/epilogue/clobber code

    /// Emit ADDI: rd = rs1 + imm
    pub fn push_addi(&mut self, rd: Gpr, rs1: Gpr, imm: i32) {
        self.emit(Inst::Addi { rd, rs1, imm });
    }

    /// Emit ADD: rd = rs1 + rs2
    pub fn push_add(&mut self, rd: Gpr, rs1: Gpr, rs2: Gpr) {
        self.emit(Inst::Add { rd, rs1, rs2 });
    }

    /// Emit SUB: rd = rs1 - rs2
    pub fn push_sub(&mut self, rd: Gpr, rs1: Gpr, rs2: Gpr) {
        self.emit(Inst::Sub { rd, rs1, rs2 });
    }

    /// Emit LW: rd = mem[rs1 + imm]
    pub fn push_lw(&mut self, rd: Gpr, rs1: Gpr, imm: i32) {
        self.emit(Inst::Lw { rd, rs1, imm });
    }

    /// Emit SW: mem[rs1 + imm] = rs2
    pub fn push_sw(&mut self, rs1: Gpr, rs2: Gpr, imm: i32) {
        self.emit(Inst::Sw { rs1, rs2, imm });
    }

    /// Emit LUI: rd = imm << 12
    pub fn push_lui(&mut self, rd: Gpr, imm: u32) {
        self.emit(Inst::Lui { rd, imm });
    }

    /// Emit JALR: rd = pc + 4; pc = rs1 + imm
    pub fn push_jalr(&mut self, rd: Gpr, rs1: Gpr, imm: i32) {
        self.emit(Inst::Jalr { rd, rs1, imm });
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

    /// Set an instruction at a specific index (for fixup).
    ///
    /// # Panics
    ///
    /// Panics if `index >= instructions.len()`.
    pub fn set_instruction(&mut self, idx: usize, inst: Inst) {
        assert!(
            idx < self.instructions.len(),
            "Instruction index {} is out of bounds (instruction count: {})",
            idx,
            self.instructions.len()
        );
        self.instructions[idx] = inst;
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

impl Default for InstBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl InstBuffer {
    /// Assert that the instructions in this buffer match the given assembly code.
    ///
    /// This is a test helper that makes it easy to verify instruction sequences
    /// by comparing against readable assembly code.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut buf = InstBuffer::new();
    /// buf.push_addi(Gpr::Sp, Gpr::Sp, -8);
    /// buf.push_sw(Gpr::Sp, Gpr::Ra, 4);
    /// buf.assert_asm(
    ///     "
    ///     addi sp, sp, -8
    ///     sw ra, 4(sp)
    /// ",
    /// );
    /// ```
    pub fn assert_asm(&self, expected_asm: &str) {
        use super::asm_parser::assemble_code;
        extern crate alloc;

        // Assemble the expected code
        let expected_bytes = assemble_code(expected_asm.trim(), None).unwrap_or_else(|e| {
            panic!(
                "Failed to assemble expected code: {}\nCode:\n{}",
                e, expected_asm
            )
        });

        // Get actual encoded instructions
        let actual_bytes = self.as_bytes();

        // Compare
        assert_eq!(
            actual_bytes.len(),
            expected_bytes.len(),
            "Instruction count mismatch. Expected {} bytes ({} instructions), got {} bytes ({} \
             instructions)",
            expected_bytes.len(),
            expected_bytes.len() / 4,
            actual_bytes.len(),
            actual_bytes.len() / 4
        );

        assert_eq!(
            actual_bytes,
            expected_bytes,
            "Instructions don't match.\nExpected:\n{}\nGot:\n{}",
            expected_asm.trim(),
            self.disassemble()
        );
    }

    /// Disassemble all instructions in this buffer to a string.
    fn disassemble(&self) -> alloc::string::String {
        use super::disasm::disassemble_instruction;
        extern crate alloc;
        use alloc::string::String;

        let mut result = String::new();
        let insts = self.instructions();
        for (i, inst) in insts.iter().enumerate() {
            if i > 0 {
                result.push_str("\n");
            }
            result.push_str(&disassemble_instruction(inst.encode()));
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Gpr, Inst};

    #[test]
    fn test_code_buffer() {
        let mut buf = InstBuffer::new();
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

    #[test]
    fn test_convenience_methods() {
        let mut buf = InstBuffer::new();

        // Test push_addi
        buf.push_addi(Gpr::A0, Gpr::A1, 42);
        let insts = buf.instructions();
        assert_eq!(insts.len(), 1);
        assert!(matches!(
            insts[0],
            Inst::Addi {
                rd: Gpr::A0,
                rs1: Gpr::A1,
                imm: 42
            }
        ));

        // Test push_lw
        buf.push_lw(Gpr::A2, Gpr::Sp, 4);
        let insts = buf.instructions();
        assert_eq!(insts.len(), 2);
        assert!(matches!(
            insts[1],
            Inst::Lw {
                rd: Gpr::A2,
                rs1: Gpr::Sp,
                imm: 4
            }
        ));

        // Test push_sw
        buf.push_sw(Gpr::Sp, Gpr::Ra, 8);
        let insts = buf.instructions();
        assert_eq!(insts.len(), 3);
        assert!(matches!(
            insts[2],
            Inst::Sw {
                rs1: Gpr::Sp,
                rs2: Gpr::Ra,
                imm: 8
            }
        ));

        // Test push_add
        buf.push_add(Gpr::A3, Gpr::A0, Gpr::A1);
        let insts = buf.instructions();
        assert_eq!(insts.len(), 4);
        assert!(matches!(
            insts[3],
            Inst::Add {
                rd: Gpr::A3,
                rs1: Gpr::A0,
                rs2: Gpr::A1
            }
        ));

        // Test push_lui
        buf.push_lui(Gpr::A4, 0x12345000);
        let insts = buf.instructions();
        assert_eq!(insts.len(), 5);
        assert!(matches!(
            insts[4],
            Inst::Lui {
                rd: Gpr::A4,
                imm: 0x12345000
            }
        ));
    }
}
