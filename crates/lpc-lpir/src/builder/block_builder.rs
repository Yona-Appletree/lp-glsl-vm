//! Block builder.

use alloc::{string::String, vec::Vec};

use crate::{
    builder::function_builder::FunctionBuilder,
    condcodes::{FloatCC, IntCC},
    dfg::{InstData, Opcode},
    entity::Block as BlockEntity,
    trapcode::TrapCode,
    Type, Value,
};

/// Builder for adding instructions to a block.
///
/// This provides a convenient API for adding instructions to a block
/// while maintaining proper SSA form.
pub struct BlockBuilder<'a> {
    /// Reference to the function builder.
    function_builder: &'a mut FunctionBuilder,
    /// Block entity being built.
    block: BlockEntity,
}

impl<'a> BlockBuilder<'a> {
    /// Create a new block builder (internal use only).
    pub fn new(function_builder: &'a mut FunctionBuilder, block: BlockEntity) -> Self {
        Self {
            function_builder,
            block,
        }
    }

    /// Add an instruction to this block.
    fn push_inst_data(&mut self, inst_data: InstData) {
        let inst = self.function_builder.function_mut().create_inst(inst_data);
        self.function_builder
            .function_mut()
            .append_inst(inst, self.block);
    }

    // Arithmetic instructions

    /// Integer add: result = arg1 + arg2
    pub fn iadd(&mut self, result: Value, arg1: Value, arg2: Value) {
        self.push_inst_data(InstData::arithmetic(Opcode::Iadd, result, arg1, arg2));
    }

    /// Integer subtract: result = arg1 - arg2
    pub fn isub(&mut self, result: Value, arg1: Value, arg2: Value) {
        self.push_inst_data(InstData::arithmetic(Opcode::Isub, result, arg1, arg2));
    }

    /// Integer multiply: result = arg1 * arg2
    pub fn imul(&mut self, result: Value, arg1: Value, arg2: Value) {
        self.push_inst_data(InstData::arithmetic(Opcode::Imul, result, arg1, arg2));
    }

    /// Integer divide: result = arg1 / arg2
    pub fn idiv(&mut self, result: Value, arg1: Value, arg2: Value) {
        self.push_inst_data(InstData::arithmetic(Opcode::Idiv, result, arg1, arg2));
    }

    /// Integer remainder: result = arg1 % arg2
    pub fn irem(&mut self, result: Value, arg1: Value, arg2: Value) {
        self.push_inst_data(InstData::arithmetic(Opcode::Irem, result, arg1, arg2));
    }

    // Comparison instructions

    /// Integer comparison: result = (arg1 cond arg2)
    pub fn icmp(&mut self, result: Value, cond: IntCC, arg1: Value, arg2: Value) {
        self.push_inst_data(InstData::comparison(
            Opcode::Icmp { cond },
            result,
            arg1,
            arg2,
        ));
    }

    /// Floating point comparison: result = (arg1 cond arg2)
    /// Note: IR-only, backend lowering not supported yet
    pub fn fcmp(&mut self, result: Value, cond: FloatCC, arg1: Value, arg2: Value) {
        self.push_inst_data(InstData::comparison(
            Opcode::Fcmp { cond },
            result,
            arg1,
            arg2,
        ));
    }

    // Convenience methods for backward compatibility

    /// Integer compare equal: result = (arg1 == arg2)
    pub fn icmp_eq(&mut self, result: Value, arg1: Value, arg2: Value) {
        self.icmp(result, IntCC::Equal, arg1, arg2);
    }

    /// Integer compare not equal: result = (arg1 != arg2)
    pub fn icmp_ne(&mut self, result: Value, arg1: Value, arg2: Value) {
        self.icmp(result, IntCC::NotEqual, arg1, arg2);
    }

    /// Integer compare less than: result = (arg1 < arg2)
    pub fn icmp_lt(&mut self, result: Value, arg1: Value, arg2: Value) {
        self.icmp(result, IntCC::SignedLessThan, arg1, arg2);
    }

    /// Integer compare less than or equal: result = (arg1 <= arg2)
    pub fn icmp_le(&mut self, result: Value, arg1: Value, arg2: Value) {
        self.icmp(result, IntCC::SignedLessThanOrEqual, arg1, arg2);
    }

    /// Integer compare greater than: result = (arg1 > arg2)
    pub fn icmp_gt(&mut self, result: Value, arg1: Value, arg2: Value) {
        self.icmp(result, IntCC::SignedGreaterThan, arg1, arg2);
    }

    /// Integer compare greater than or equal: result = (arg1 >= arg2)
    pub fn icmp_ge(&mut self, result: Value, arg1: Value, arg2: Value) {
        self.icmp(result, IntCC::SignedGreaterThanOrEqual, arg1, arg2);
    }

    // Constant instructions

    /// Integer constant: result = value
    pub fn iconst(&mut self, result: Value, value: i64) {
        use crate::dfg::Immediate;
        self.push_inst_data(InstData::constant(result, Immediate::I64(value)));
    }

    /// Floating point constant: result = value
    pub fn fconst(&mut self, result: Value, value: f32) {
        use crate::dfg::Immediate;
        self.push_inst_data(InstData::constant(
            result,
            Immediate::F32Bits(value.to_bits()),
        ));
    }

    // Control flow instructions

    /// Jump to target block.
    pub fn jump(&mut self, target: BlockEntity, args: &[Value]) {
        self.push_inst_data(InstData::jump(target, args.to_vec()));
    }

    /// Conditional branch: if condition, jump to target_true, else target_false
    pub fn br(
        &mut self,
        condition: Value,
        target_true: BlockEntity,
        args_true: &[Value],
        target_false: BlockEntity,
        args_false: &[Value],
    ) {
        self.push_inst_data(InstData::branch(
            condition,
            target_true,
            args_true.to_vec(),
            target_false,
            args_false.to_vec(),
        ));
    }

    /// Return with values.
    pub fn return_(&mut self, values: &[Value]) {
        self.push_inst_data(InstData::return_(values.to_vec()));
    }

    // Memory instructions

    /// Load from memory: result = mem[address]
    pub fn load(&mut self, result: Value, address: Value, ty: Type) {
        self.push_inst_data(InstData::load(result, address, ty));
    }

    /// Store to memory: mem[address] = value
    pub fn store(&mut self, address: Value, value: Value, ty: Type) {
        self.push_inst_data(InstData::store(address, value, ty));
    }

    /// Function call: results = callee(args...)
    pub fn call(&mut self, callee: String, args: Vec<Value>, results: Vec<Value>) {
        self.push_inst_data(InstData::call(callee, args, results));
    }

    /// System call: syscall(number, args...)
    pub fn syscall(&mut self, number: i32, args: Vec<Value>) {
        self.push_inst_data(InstData::syscall(number, args));
    }

    /// Halt execution (ebreak)
    pub fn halt(&mut self) {
        self.push_inst_data(InstData::halt());
    }

    // Trap instructions

    /// Unconditional trap: terminate execution with trap code
    pub fn trap(&mut self, code: TrapCode) {
        self.push_inst_data(InstData::trap(code));
    }

    /// Trap if condition is zero: if condition == 0, trap with code
    pub fn trapz(&mut self, condition: Value, code: TrapCode) {
        self.push_inst_data(InstData::trapz(condition, code));
    }

    /// Trap if condition is non-zero: if condition != 0, trap with code
    pub fn trapnz(&mut self, condition: Value, code: TrapCode) {
        self.push_inst_data(InstData::trapnz(condition, code));
    }
}
