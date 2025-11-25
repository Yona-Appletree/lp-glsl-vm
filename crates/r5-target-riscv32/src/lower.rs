//! Lower IR to RISC-V 32-bit instructions.

use alloc::{collections::BTreeMap, string::String};

use r5_ir::{Function, Inst, Module, Value};
use riscv32_encoder::{self, Gpr};

use crate::{emit::CodeBuffer, regalloc::SimpleRegAllocator};

/// Lower IR to RISC-V 32-bit code.
pub struct Lowerer {
    regalloc: SimpleRegAllocator,
    /// Module context for function calls (optional).
    module: Option<Module>,
    /// Function addresses (for call relocations).
    function_addresses: BTreeMap<String, u32>,
}

impl Lowerer {
    /// Create a new lowerer.
    pub fn new() -> Self {
        Self {
            regalloc: SimpleRegAllocator::new(),
            module: None,
            function_addresses: alloc::collections::BTreeMap::new(),
        }
    }

    /// Set the module context for function calls.
    pub fn set_module(&mut self, module: Module) {
        self.module = Some(module);
    }

    /// Set a function address for call relocations.
    pub fn set_function_address(&mut self, name: String, address: u32) {
        self.function_addresses.insert(name, address);
    }

    /// Lower a function to RISC-V 32-bit code.
    pub fn lower_function(&mut self, func: &Function) -> CodeBuffer {
        let mut code = CodeBuffer::new();
        self.regalloc.clear();

        // Map function parameters to argument registers (a0-a7)
        // The entry block's parameters correspond to function parameters
        if let Some(entry_block) = func.blocks.first() {
            for (i, param) in entry_block.params.iter().enumerate() {
                let arg_reg = match i {
                    0 => Gpr::A0,
                    1 => Gpr::A1,
                    2 => Gpr::A2,
                    3 => Gpr::A3,
                    4 => Gpr::A4,
                    5 => Gpr::A5,
                    6 => Gpr::A6,
                    7 => Gpr::A7,
                    _ => break, // More than 8 parameters not supported yet
                };
                // Map the parameter value to the argument register
                self.regalloc.map_value_to_register(*param, arg_reg);
            }
        }

        // For now, just lower each instruction in order
        // TODO: Handle basic blocks, control flow, etc.
        for block in &func.blocks {
            // Handle block parameters (phi nodes) - allocate registers if not already mapped
            for param in &block.params {
                if !self.regalloc.is_mapped(*param) {
                    self.regalloc.allocate(*param);
                }
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
            Inst::Call {
                callee,
                args,
                results,
            } => {
                self.lower_call(code, callee, args, results);
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

    /// Lower `call` instruction: results = callee(args...)
    fn lower_call(
        &mut self,
        code: &mut CodeBuffer,
        _callee: &str,
        args: &[Value],
        results: &[Value],
    ) {
        // Save caller-saved registers if needed (for now, assume we don't need to)
        // TODO: Implement proper caller-saved register handling

        // Set up arguments in a0-a7
        for (i, arg) in args.iter().take(8).enumerate() {
            let arg_reg = match i {
                0 => Gpr::A0,
                1 => Gpr::A1,
                2 => Gpr::A2,
                3 => Gpr::A3,
                4 => Gpr::A4,
                5 => Gpr::A5,
                6 => Gpr::A6,
                7 => Gpr::A7,
                _ => break,
            };

            // Get the register for the argument value
            let arg_value_reg = self.regalloc.get(*arg).unwrap_or_else(|| {
                // If not allocated, allocate it now
                self.regalloc.allocate(*arg)
            });

            // Move argument to the appropriate argument register
            if arg_value_reg.num() != arg_reg.num() {
                code.emit(riscv32_encoder::add(arg_reg, arg_value_reg, Gpr::ZERO));
            }
        }

        // Call the function
        // For now, we'll use a placeholder that needs to be fixed up
        // We'll emit jal ra, offset where offset will be calculated during linking
        let call_offset = 0; // Placeholder - will be fixed up
        code.emit(riscv32_encoder::jal(Gpr::RA, call_offset));

        // Get return value(s) from a0, a1, etc.
        for (i, result) in results.iter().enumerate() {
            let ret_reg = match i {
                0 => Gpr::A0,
                1 => Gpr::A1,
                2 => Gpr::A2,
                3 => Gpr::A3,
                4 => Gpr::A4,
                5 => Gpr::A5,
                6 => Gpr::A6,
                7 => Gpr::A7,
                _ => break,
            };

            // Map the result value to the return register
            self.regalloc.map_value_to_register(*result, ret_reg);
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
