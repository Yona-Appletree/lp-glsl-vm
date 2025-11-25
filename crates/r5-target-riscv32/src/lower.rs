//! Lower IR to RISC-V 32-bit instructions.

use r5_ir::{Function, Inst, Value};
use riscv32_encoder::{self, Gpr};

use crate::{emit::CodeBuffer, regalloc::SimpleRegAllocator};

/// Lower IR to RISC-V 32-bit code.
pub struct Lowerer {
    regalloc: SimpleRegAllocator,
}

impl Lowerer {
    /// Create a new lowerer.
    pub fn new() -> Self {
        Self {
            regalloc: SimpleRegAllocator::new(),
        }
    }

    /// Lower a function to RISC-V 32-bit code.
    pub fn lower_function(&mut self, func: &Function) -> CodeBuffer {
        let mut code = CodeBuffer::new();
        self.regalloc.clear();

        // For now, just lower each instruction in order
        // TODO: Handle basic blocks, control flow, etc.
        for block in &func.blocks {
            // Handle block parameters (phi nodes) - for now, just allocate registers
            for param in &block.params {
                self.regalloc.allocate(*param);
            }

            // Lower each instruction
            for inst in &block.insts {
                self.lower_inst(&mut code, func, inst);
            }
        }

        code
    }

    /// Lower a single instruction.
    fn lower_inst(&mut self, code: &mut CodeBuffer, _func: &Function, inst: &Inst) {
        match inst {
            Inst::Iadd { result, arg1, arg2 } => {
                self.lower_iadd(code, *result, *arg1, *arg2);
            }
            Inst::Isub { result, arg1, arg2 } => {
                self.lower_isub(code, *result, *arg1, *arg2);
            }
            Inst::Imul { result, arg1, arg2 } => {
                self.lower_imul(code, *result, *arg1, *arg2);
            }
            Inst::Iconst { result, value } => {
                self.lower_iconst(code, *result, *value);
            }
            Inst::Return { values } => {
                self.lower_return(code, values);
            }
            _ => {
                // TODO: Handle more instructions
                panic!("Unsupported instruction: {:?}", inst);
            }
        }
    }

    /// Lower `iadd` instruction: result = arg1 + arg2
    fn lower_iadd(&mut self, code: &mut CodeBuffer, result: Value, arg1: Value, arg2: Value) {
        let reg1 = self.regalloc.allocate(arg1);
        let reg2 = self.regalloc.allocate(arg2);
        let reg_result = self.regalloc.allocate(result);

        // Use add for register-register
        code.emit(riscv32_encoder::add(reg_result, reg1, reg2));
    }

    /// Lower `isub` instruction: result = arg1 - arg2
    fn lower_isub(&mut self, code: &mut CodeBuffer, result: Value, arg1: Value, arg2: Value) {
        let reg1 = self.regalloc.allocate(arg1);
        let reg2 = self.regalloc.allocate(arg2);
        let reg_result = self.regalloc.allocate(result);

        // Use sub for register-register
        code.emit(riscv32_encoder::sub(reg_result, reg1, reg2));
    }

    /// Lower `imul` instruction: result = arg1 * arg2
    fn lower_imul(&mut self, code: &mut CodeBuffer, result: Value, arg1: Value, arg2: Value) {
        let reg1 = self.regalloc.allocate(arg1);
        let reg2 = self.regalloc.allocate(arg2);
        let reg_result = self.regalloc.allocate(result);

        // Use mul for register-register (M extension)
        code.emit(riscv32_encoder::mul(reg_result, reg1, reg2));
    }

    /// Lower `iconst` instruction: result = value
    fn lower_iconst(&mut self, code: &mut CodeBuffer, result: Value, value: i64) {
        let reg_result = self.regalloc.allocate(result);
        let imm_i32 = value as i32;

        // For small constants, use addi with x0
        if imm_i32 >= -2048 && imm_i32 < 2048 {
            code.emit(riscv32_encoder::addi(reg_result, Gpr::ZERO, imm_i32));
            return;
        }

        // For larger constants, use lui + addi
        let imm_u32 = value as u32;
        let imm_hi = (imm_u32 >> 12) & 0xfffff;
        let imm_lo = (imm_u32 & 0xfff) as i32;

        // Sign-extend the low 12 bits if needed
        let imm_lo_signed = if imm_lo & 0x800 != 0 {
            imm_lo | (-4096i32) // 0xfffff000 as i32
        } else {
            imm_lo
        };

        code.emit(riscv32_encoder::lui(reg_result, imm_hi << 12));
        if imm_lo_signed != 0 {
            code.emit(riscv32_encoder::addi(reg_result, reg_result, imm_lo_signed));
        }
    }

    /// Lower `return` instruction
    fn lower_return(&mut self, code: &mut CodeBuffer, values: &[Value]) {
        // For now, just return (jalr x0, x1, 0)
        // TODO: Handle return values properly (move to a0/a1)
        if !values.is_empty() {
            // Move first return value to a0
            let ret_val = values[0];
            let ret_reg = self.regalloc.get(ret_val).unwrap_or_else(|| {
                // If not allocated, allocate it now
                self.regalloc.allocate(ret_val)
            });
            if ret_reg.num() != 10 {
                // Move to a0 if not already there
                code.emit(riscv32_encoder::add(Gpr::A0, ret_reg, Gpr::ZERO));
            }
        }
        // Return: jalr x0, x1, 0
        code.emit(riscv32_encoder::jalr(Gpr::ZERO, Gpr::RA, 0));
    }
}

impl Default for Lowerer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use r5_ir::{Block, Signature, Type};

    use super::*;

    #[test]
    fn test_lower_simple_add() {
        // Create a simple function: fn add(a: i32, b: i32) -> i32 { a + b }
        let sig = Signature::new(vec![Type::I32, Type::I32], vec![Type::I32]);
        let mut func = Function::new(sig);

        let mut block = Block::new();
        // Parameters would be passed as block params in real usage
        let a = Value::new(0);
        let b = Value::new(1);
        let result = Value::new(2);

        block.push_inst(Inst::Iadd {
            result,
            arg1: a,
            arg2: b,
        });
        block.push_inst(Inst::Return {
            values: vec![result],
        });

        func.add_block(block);

        // Lower the function
        let mut lowerer = Lowerer::new();
        let code = lowerer.lower_function(&func);

        // Should have at least an add instruction
        assert!(code.instruction_count() > 0);
    }
}
