//! Lower IR to RISC-V 32-bit instructions.

use alloc::{collections::BTreeMap, string::String, vec::Vec};

use r5_ir::{Function, Inst, Module, Value};
use riscv32_encoder::{self, Gpr};

use crate::{emit::CodeBuffer, regalloc::SimpleRegAllocator};

/// A relocation that needs to be fixed up.
#[derive(Debug, Clone)]
pub struct Relocation {
    /// Offset in the code buffer where the instruction is
    pub offset: usize,
    /// Name of the function being called
    pub callee: String,
}

/// Lower IR to RISC-V 32-bit code.
pub struct Lowerer {
    regalloc: SimpleRegAllocator,
    /// Module context for function calls (optional).
    module: Option<Module>,
    /// Function addresses (for call relocations).
    function_addresses: BTreeMap<String, u32>,
    /// Relocations that need to be fixed up (call sites).
    relocations: Vec<Relocation>,
}

impl Lowerer {
    /// Create a new lowerer.
    pub fn new() -> Self {
        Self {
            regalloc: SimpleRegAllocator::new(),
            module: None,
            function_addresses: BTreeMap::new(),
            relocations: Vec::new(),
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

    /// Get relocations that need to be fixed up.
    pub fn relocations(&self) -> &[Relocation] {
        &self.relocations
    }

    /// Clear relocations (for next function).
    pub fn clear_relocations(&mut self) {
        self.relocations.clear();
    }

    /// Map function parameters to argument registers (a0-a7).
    fn map_parameters_to_registers(regalloc: &mut SimpleRegAllocator, entry_block: &r5_ir::Block) {
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
            regalloc.map_value_to_register(*param, arg_reg);
        }
    }

    /// Estimate the size of an instruction in bytes (for block address computation).
    fn estimate_instruction_size(inst: &Inst) -> usize {
        match inst {
            Inst::Iadd { .. }
            | Inst::Isub { .. }
            | Inst::Imul { .. }
            | Inst::Iconst { .. }
            | Inst::Jump { .. }
            | Inst::Return { .. }
            | Inst::Halt => 4, // Single instruction
            Inst::Call { .. } => 4, // jal instruction (plus argument moves, but we don't count those here)
            Inst::Syscall { number, args } => {
                // Estimate: argument moves + syscall number setup + ecall
                let mut size = args.len().min(8) * 4; // Each arg may need a move
                if *number >= -2048 && *number < 2048 {
                    size += 4; // addi
                } else {
                    size += 8; // lui + addi
                }
                size += 4; // ecall
                size
            }
            _ => 4, // Default: single instruction
        }
    }

    /// Compute block start addresses using a first pass.
    /// This estimates instruction sizes to determine where each block will start.
    fn compute_block_addresses(func: &Function) -> Vec<usize> {
        let mut block_starts = Vec::new();
        let mut temp_code = CodeBuffer::new();
        let mut temp_regalloc = SimpleRegAllocator::new();

        // Map function parameters for first pass
        if let Some(entry_block) = func.blocks.first() {
            Self::map_parameters_to_registers(&mut temp_regalloc, entry_block);
        }

        for block in &func.blocks {
            block_starts.push(temp_code.len());

            // Handle block parameters
            for param in &block.params {
                if !temp_regalloc.is_mapped(*param) {
                    temp_regalloc.allocate(*param);
                }
            }

            // Estimate instruction sizes
            for inst in &block.insts {
                let size = Self::estimate_instruction_size(inst);
                // Allocate registers for values used/produced
                match inst {
                    Inst::Iadd { result, arg1, arg2 } => {
                        let _ = temp_regalloc.allocate(*arg1);
                        let _ = temp_regalloc.allocate(*arg2);
                        let _ = temp_regalloc.allocate(*result);
                    }
                    Inst::Iconst { result, .. } => {
                        let _ = temp_regalloc.allocate(*result);
                    }
                    Inst::Call { args, results, .. } => {
                        for arg in args {
                            let _ = temp_regalloc.allocate(*arg);
                        }
                        for res in results {
                            let _ = temp_regalloc.allocate(*res);
                        }
                    }
                    Inst::Syscall { args, .. } => {
                        for arg in args {
                            let _ = temp_regalloc.allocate(*arg);
                        }
                    }
                    _ => {}
                }
                // Emit placeholder bytes
                for _ in 0..size {
                    temp_code.emit(0);
                }
            }
        }

        block_starts
    }

    /// Lower a function to RISC-V 32-bit code.
    pub fn lower_function(&mut self, func: &Function) -> CodeBuffer {
        let mut code = CodeBuffer::new();
        self.regalloc.clear();

        // Two-pass approach: first pass to compute block addresses, second pass to emit code
        // First pass: compute block start addresses
        let mut block_starts = Self::compute_block_addresses(func);

        // Second pass: emit actual code with correct jump offsets
        self.regalloc.clear();
        if let Some(entry_block) = func.blocks.first() {
            Self::map_parameters_to_registers(&mut self.regalloc, entry_block);
        }

        for (block_idx, block) in func.blocks.iter().enumerate() {
            // Update block start with actual address
            block_starts[block_idx] = code.len();

            // Handle block parameters
            for param in &block.params {
                if !self.regalloc.is_mapped(*param) {
                    self.regalloc.allocate(*param);
                }
            }

            // Lower each instruction
            for inst in &block.insts {
                self.lower_inst(&mut code, func, inst, block_idx, &block_starts);
            }
        }

        code
    }

    /// Lower a single instruction.
    fn lower_inst(
        &mut self,
        code: &mut CodeBuffer,
        _func: &Function,
        inst: &Inst,
        current_block: usize,
        block_starts: &[usize],
    ) {
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
            Inst::Syscall { number, args } => {
                self.lower_syscall(code, *number, args);
            }
            Inst::Jump { target } => {
                self.lower_jump(code, *target as usize, current_block, block_starts);
            }
            Inst::Return { values } => {
                self.lower_return(code, values);
            }
            Inst::Halt => {
                self.lower_halt(code);
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
        callee: &str,
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
        // Emit jal ra, 0 as placeholder - will be fixed up during linking
        let jal_offset = code.len();
        code.emit(riscv32_encoder::jal(Gpr::RA, 0));

        // Record relocation for later fixup
        self.relocations.push(Relocation {
            offset: jal_offset,
            callee: String::from(callee),
        });

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

    /// Lower `syscall` instruction
    fn lower_syscall(&mut self, code: &mut CodeBuffer, number: i32, args: &[Value]) {
        // Set up syscall arguments in a0-a7
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
            let arg_value_reg = self
                .regalloc
                .get(*arg)
                .unwrap_or_else(|| self.regalloc.allocate(*arg));

            // Move argument to the appropriate register
            if arg_value_reg.num() != arg_reg.num() {
                code.emit(riscv32_encoder::add(arg_reg, arg_value_reg, Gpr::ZERO));
            }
        }

        // Set syscall number in a7
        if number >= -2048 && number < 2048 {
            code.emit(riscv32_encoder::addi(Gpr::A7, Gpr::ZERO, number));
        } else {
            // For larger numbers, use lui + addi
            let num_u32 = number as u32;
            let imm_hi = (num_u32 >> 12) & 0xfffff;
            let imm_lo = (num_u32 & 0xfff) as i32;
            let imm_lo_signed = if imm_lo & 0x800 != 0 {
                imm_lo | (-4096i32)
            } else {
                imm_lo
            };
            code.emit(riscv32_encoder::lui(Gpr::A7, imm_hi << 12));
            if imm_lo_signed != 0 {
                code.emit(riscv32_encoder::addi(Gpr::A7, Gpr::A7, imm_lo_signed));
            }
        }

        // Emit ecall
        code.emit(riscv32_encoder::ecall());
    }

    /// Lower `jump` instruction
    fn lower_jump(
        &mut self,
        code: &mut CodeBuffer,
        target_block: usize,
        current_block: usize,
        block_starts: &[usize],
    ) {
        // If jumping to the same block, emit an infinite loop (halt behavior).
        // This is intentional: `jal zero, 0` jumps to itself, creating a halt loop.
        // This is used for functions that should never return (e.g., main loops).
        if target_block == current_block {
            code.emit(riscv32_encoder::jal(Gpr::ZERO, 0));
            return;
        }

        // Calculate PC-relative offset to target block
        if target_block >= block_starts.len() || current_block >= block_starts.len() {
            panic!(
                "Invalid block index (target={}, current={}, num_blocks={})",
                target_block,
                current_block,
                block_starts.len()
            );
        }

        let target_addr = block_starts[target_block];
        let current_pc = code.len();
        let offset = target_addr as i32 - current_pc as i32;

        // Emit jal zero, offset (unconditional jump)
        code.emit(riscv32_encoder::jal(Gpr::ZERO, offset));
    }

    /// Lower `halt` instruction
    fn lower_halt(&mut self, code: &mut CodeBuffer) {
        code.emit(riscv32_encoder::ebreak());
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
        Self {
            regalloc: SimpleRegAllocator::new(),
            module: None,
            function_addresses: BTreeMap::new(),
            relocations: Vec::new(),
        }
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

    #[test]
    fn test_lower_syscall_small_number() {
        let sig = Signature::new(vec![Type::I32], vec![]);
        let mut func = Function::new(sig);
        let mut block = Block::new();

        let arg = Value::new(0);
        block.params.push(arg);
        block.push_inst(Inst::Syscall {
            number: 42,
            args: vec![arg],
        });
        block.push_inst(Inst::Halt);

        func.add_block(block);

        let mut lowerer = Lowerer::new();
        let code = lowerer.lower_function(&func);

        // Should have: arg is already in a0 (no move needed), addi a7, ecall, ebreak
        // Or: move arg to a0, addi a7, ecall, ebreak
        // Minimum: addi a7, ecall, ebreak = 3 instructions
        assert!(code.instruction_count() >= 3);
    }

    #[test]
    fn test_lower_syscall_large_number() {
        let sig = Signature::new(vec![Type::I32], vec![]);
        let mut func = Function::new(sig);
        let mut block = Block::new();

        let arg = Value::new(0);
        block.params.push(arg);
        // Use a large syscall number that requires lui + addi
        block.push_inst(Inst::Syscall {
            number: 0x12345,
            args: vec![arg],
        });
        block.push_inst(Inst::Halt);

        func.add_block(block);

        let mut lowerer = Lowerer::new();
        let code = lowerer.lower_function(&func);

        // Should have: arg is already in a0 (no move needed), lui a7, addi a7, ecall, ebreak
        // Or: move arg to a0, lui a7, addi a7, ecall, ebreak
        // Minimum: lui a7, addi a7, ecall, ebreak = 4 instructions
        assert!(code.instruction_count() >= 4);
    }

    #[test]
    fn test_lower_halt() {
        let sig = Signature::empty();
        let mut func = Function::new(sig);
        let mut block = Block::new();

        block.push_inst(Inst::Halt);

        func.add_block(block);

        let mut lowerer = Lowerer::new();
        let code = lowerer.lower_function(&func);

        // Should have exactly one instruction: ebreak
        assert_eq!(code.instruction_count(), 1);
        // Verify it's ebreak (0x00100073)
        let bytes = code.as_bytes();
        assert_eq!(bytes.len(), 4);
        let inst = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        assert_eq!(inst, 0x00100073);
    }

    #[test]
    fn test_lower_jump_forward() {
        let sig = Signature::empty();
        let mut func = Function::new(sig);

        // Block 0: jump to block 1
        let mut block0 = Block::new();
        block0.push_inst(Inst::Jump { target: 1 });
        func.add_block(block0);

        // Block 1: halt
        let mut block1 = Block::new();
        block1.push_inst(Inst::Halt);
        func.add_block(block1);

        let mut lowerer = Lowerer::new();
        let code = lowerer.lower_function(&func);

        // Should have at least 2 instructions: jal + ebreak
        assert!(code.instruction_count() >= 2);
    }

    #[test]
    fn test_lower_jump_same_block() {
        let sig = Signature::empty();
        let mut func = Function::new(sig);

        // Block that jumps to itself (infinite loop)
        let mut block = Block::new();
        block.push_inst(Inst::Jump { target: 0 });
        func.add_block(block);

        let mut lowerer = Lowerer::new();
        let code = lowerer.lower_function(&func);

        // Should have jal zero, 0 (infinite loop)
        assert!(code.instruction_count() >= 1);
        let bytes = code.as_bytes();
        assert!(bytes.len() >= 4);
    }

    #[test]
    fn test_relocation_recording() {
        let sig = Signature::new(vec![Type::I32], vec![Type::I32]);
        let mut func = Function::new(sig);
        let mut block = Block::new();

        let arg = Value::new(0);
        let result = Value::new(1);
        block.params.push(arg);
        block.push_inst(Inst::Call {
            callee: alloc::string::String::from("other_func"),
            args: vec![arg],
            results: vec![result],
        });
        block.push_inst(Inst::Return {
            values: vec![result],
        });

        func.add_block(block);

        let mut lowerer = Lowerer::new();
        let _code = lowerer.lower_function(&func);

        // Should have recorded a relocation
        let relocations = lowerer.relocations();
        assert_eq!(relocations.len(), 1);
        assert_eq!(relocations[0].callee, "other_func");
    }

    #[test]
    fn test_compute_block_addresses() {
        let sig = Signature::empty();
        let mut func = Function::new(sig);

        // Block 0: iconst + jump
        let mut block0 = Block::new();
        let v = Value::new(0);
        block0.push_inst(Inst::Iconst {
            result: v,
            value: 42,
        });
        block0.push_inst(Inst::Jump { target: 1 });
        func.add_block(block0);

        // Block 1: halt
        let mut block1 = Block::new();
        block1.push_inst(Inst::Halt);
        func.add_block(block1);

        let block_starts = Lowerer::compute_block_addresses(&func);

        // Should have 2 block starts
        assert_eq!(block_starts.len(), 2);
        // Block 0 should start at 0
        assert_eq!(block_starts[0], 0);
        // Block 1 should start after block 0's instructions
        assert!(block_starts[1] > block_starts[0]);
    }

    #[test]
    fn test_map_parameters_to_registers() {
        let sig = Signature::new(vec![Type::I32, Type::I32, Type::I32], vec![Type::I32]);
        let mut func = Function::new(sig);
        let mut block = Block::new();

        let p0 = Value::new(0);
        let p1 = Value::new(1);
        let p2 = Value::new(2);
        block.params.push(p0);
        block.params.push(p1);
        block.params.push(p2);

        func.add_block(block);

        let mut regalloc = SimpleRegAllocator::new();
        if let Some(entry_block) = func.blocks.first() {
            Lowerer::map_parameters_to_registers(&mut regalloc, entry_block);
        }

        // Parameters should be mapped to a0, a1, a2
        assert_eq!(regalloc.get(p0).unwrap().num(), 10); // a0
        assert_eq!(regalloc.get(p1).unwrap().num(), 11); // a1
        assert_eq!(regalloc.get(p2).unwrap().num(), 12); // a2
    }
}
