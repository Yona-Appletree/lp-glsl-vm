//! Block builder.

use alloc::{string::String, vec::Vec};

use crate::{builder::function_builder::FunctionBuilder, Inst, Type, Value};

/// Builder for adding instructions to a block.
///
/// This provides a convenient API for adding instructions to a block
/// while maintaining proper SSA form.
pub struct BlockBuilder<'a> {
    /// Reference to the function builder.
    function_builder: &'a mut FunctionBuilder,
    /// Index of the block being built.
    block_index: usize,
}

impl<'a> BlockBuilder<'a> {
    /// Create a new block builder (internal use only).
    pub fn new(function_builder: &'a mut FunctionBuilder, block_index: usize) -> Self {
        Self {
            function_builder,
            block_index,
        }
    }

    /// Add an instruction to this block.
    fn push_inst(&mut self, inst: Inst) {
        let block = self
            .function_builder
            .function_mut()
            .block_mut(self.block_index)
            .expect("Block should exist");
        block.push_inst(inst);
    }

    // Arithmetic instructions

    /// Integer add: result = arg1 + arg2
    pub fn iadd(&mut self, result: Value, arg1: Value, arg2: Value) {
        self.push_inst(Inst::Iadd { result, arg1, arg2 });
    }

    /// Integer subtract: result = arg1 - arg2
    pub fn isub(&mut self, result: Value, arg1: Value, arg2: Value) {
        self.push_inst(Inst::Isub { result, arg1, arg2 });
    }

    /// Integer multiply: result = arg1 * arg2
    pub fn imul(&mut self, result: Value, arg1: Value, arg2: Value) {
        self.push_inst(Inst::Imul { result, arg1, arg2 });
    }

    /// Integer divide: result = arg1 / arg2
    pub fn idiv(&mut self, result: Value, arg1: Value, arg2: Value) {
        self.push_inst(Inst::Idiv { result, arg1, arg2 });
    }

    /// Integer remainder: result = arg1 % arg2
    pub fn irem(&mut self, result: Value, arg1: Value, arg2: Value) {
        self.push_inst(Inst::Irem { result, arg1, arg2 });
    }

    // Comparison instructions

    /// Integer compare equal: result = (arg1 == arg2)
    pub fn icmp_eq(&mut self, result: Value, arg1: Value, arg2: Value) {
        self.push_inst(Inst::IcmpEq { result, arg1, arg2 });
    }

    /// Integer compare not equal: result = (arg1 != arg2)
    pub fn icmp_ne(&mut self, result: Value, arg1: Value, arg2: Value) {
        self.push_inst(Inst::IcmpNe { result, arg1, arg2 });
    }

    /// Integer compare less than: result = (arg1 < arg2)
    pub fn icmp_lt(&mut self, result: Value, arg1: Value, arg2: Value) {
        self.push_inst(Inst::IcmpLt { result, arg1, arg2 });
    }

    /// Integer compare less than or equal: result = (arg1 <= arg2)
    pub fn icmp_le(&mut self, result: Value, arg1: Value, arg2: Value) {
        self.push_inst(Inst::IcmpLe { result, arg1, arg2 });
    }

    /// Integer compare greater than: result = (arg1 > arg2)
    pub fn icmp_gt(&mut self, result: Value, arg1: Value, arg2: Value) {
        self.push_inst(Inst::IcmpGt { result, arg1, arg2 });
    }

    /// Integer compare greater than or equal: result = (arg1 >= arg2)
    pub fn icmp_ge(&mut self, result: Value, arg1: Value, arg2: Value) {
        self.push_inst(Inst::IcmpGe { result, arg1, arg2 });
    }

    // Constant instructions

    /// Integer constant: result = value
    pub fn iconst(&mut self, result: Value, value: i64) {
        self.push_inst(Inst::Iconst { result, value });
    }

    /// Floating point constant: result = value
    pub fn fconst(&mut self, result: Value, value: f32) {
        self.push_inst(Inst::Fconst {
            result,
            value_bits: value.to_bits(), // f32::to_bits() returns u32
        });
    }

    // Control flow instructions

    /// Jump to target block.
    pub fn jump(&mut self, target: usize, args: &[Value]) {
        self.push_inst(Inst::Jump {
            target: target as u32,
            args: args.to_vec(),
        });
    }

    /// Conditional branch: if condition, jump to target_true, else target_false
    pub fn br(
        &mut self,
        condition: Value,
        target_true: usize,
        args_true: &[Value],
        target_false: usize,
        args_false: &[Value],
    ) {
        self.push_inst(Inst::Br {
            condition,
            target_true: target_true as u32,
            args_true: args_true.to_vec(),
            target_false: target_false as u32,
            args_false: args_false.to_vec(),
        });
    }

    /// Return with values.
    pub fn return_(&mut self, values: &[Value]) {
        self.push_inst(Inst::Return {
            values: values.to_vec(),
        });
    }

    // Memory instructions

    /// Load from memory: result = mem[address]
    pub fn load(&mut self, result: Value, address: Value, ty: Type) {
        self.push_inst(Inst::Load {
            result,
            address,
            ty,
        });
    }

    /// Store to memory: mem[address] = value
    pub fn store(&mut self, address: Value, value: Value, ty: Type) {
        self.push_inst(Inst::Store { address, value, ty });
    }

    /// Function call: results = callee(args...)
    pub fn call(&mut self, callee: String, args: Vec<Value>, results: Vec<Value>) {
        self.push_inst(Inst::Call {
            callee,
            args,
            results,
        });
    }

    /// System call: syscall(number, args...)
    pub fn syscall(&mut self, number: i32, args: Vec<Value>) {
        self.push_inst(Inst::Syscall { number, args });
    }

    /// Halt execution (ebreak)
    pub fn halt(&mut self) {
        self.push_inst(Inst::Halt);
    }
}
