//! Trait-based instruction builders.
//!
//! This module provides a trait-based builder API similar to Cranelift's,
//! allowing for extensible and type-safe instruction construction.

use crate::{
    condcodes::{FloatCC, IntCC},
    dfg::{Immediate, InstData, Opcode, DFG},
    entity::{Block, Inst as InstEntity},
    trapcode::TrapCode,
    types::Type,
    value::Value,
};

/// Base trait for instruction builders.
///
/// The `InstBuilderBase` trait provides the basic functionality required by
/// the methods of the `InstBuilder` trait. These methods should not normally
/// be used directly. Use the methods in the `InstBuilder` trait instead.
///
/// Any data type that implements `InstBuilderBase` also gets all the methods
/// of the `InstBuilder` trait.
pub trait InstBuilderBase<'f>: Sized {
    /// Get an immutable reference to the data flow graph that will hold the
    /// constructed instructions.
    fn data_flow_graph(&self) -> &DFG;

    /// Get a mutable reference to the data flow graph that will hold the
    /// constructed instructions.
    fn data_flow_graph_mut(&mut self) -> &mut DFG;

    /// Build an instruction and return its entity, consuming the builder.
    ///
    /// This creates the instruction in the DFG and returns both the instruction
    /// entity and a mutable reference to the DFG for further operations.
    fn build(self, data: InstData) -> (InstEntity, &'f mut DFG);
}

/// Instruction builder trait with methods for each instruction type.
///
/// This trait is manually defined (not generated from a meta-language) and
/// provides type-safe methods for constructing all instruction types.
pub trait InstBuilder<'f>: InstBuilderBase<'f> {
    // Arithmetic instructions - return Value

    /// Integer add: result = arg1 + arg2
    fn iadd(self, arg1: Value, arg2: Value) -> Value
    where
        Self: Sized,
    {
        let next_idx = self.data_flow_graph().next_value_index();
        let result = Value::new(next_idx);
        let _ = self.build(InstData::arithmetic(Opcode::Iadd, result, arg1, arg2));
        result
    }

    /// Integer subtract: result = arg1 - arg2
    fn isub(self, arg1: Value, arg2: Value) -> Value
    where
        Self: Sized,
    {
        let next_idx = self.data_flow_graph().next_value_index();
        let result = Value::new(next_idx);
        let _ = self.build(InstData::arithmetic(Opcode::Isub, result, arg1, arg2));
        result
    }

    /// Integer multiply: result = arg1 * arg2
    fn imul(self, arg1: Value, arg2: Value) -> Value
    where
        Self: Sized,
    {
        let next_idx = self.data_flow_graph().next_value_index();
        let result = Value::new(next_idx);
        let _ = self.build(InstData::arithmetic(Opcode::Imul, result, arg1, arg2));
        result
    }

    /// Integer divide: result = arg1 / arg2
    fn idiv(self, arg1: Value, arg2: Value) -> Value
    where
        Self: Sized,
    {
        let next_idx = self.data_flow_graph().next_value_index();
        let result = Value::new(next_idx);
        let _ = self.build(InstData::arithmetic(Opcode::Idiv, result, arg1, arg2));
        result
    }

    /// Integer remainder: result = arg1 % arg2
    fn irem(self, arg1: Value, arg2: Value) -> Value
    where
        Self: Sized,
    {
        let next_idx = self.data_flow_graph().next_value_index();
        let result = Value::new(next_idx);
        let _ = self.build(InstData::arithmetic(Opcode::Irem, result, arg1, arg2));
        result
    }

    // Bitwise instructions - return Value

    /// Bitwise AND: result = arg1 & arg2
    fn iand(self, arg1: Value, arg2: Value) -> Value
    where
        Self: Sized,
    {
        let next_idx = self.data_flow_graph().next_value_index();
        let result = Value::new(next_idx);
        let _ = self.build(InstData::bitwise(Opcode::Iand, result, arg1, arg2));
        result
    }

    /// Bitwise OR: result = arg1 | arg2
    fn ior(self, arg1: Value, arg2: Value) -> Value
    where
        Self: Sized,
    {
        let next_idx = self.data_flow_graph().next_value_index();
        let result = Value::new(next_idx);
        let _ = self.build(InstData::bitwise(Opcode::Ior, result, arg1, arg2));
        result
    }

    /// Bitwise XOR: result = arg1 ^ arg2
    fn ixor(self, arg1: Value, arg2: Value) -> Value
    where
        Self: Sized,
    {
        let next_idx = self.data_flow_graph().next_value_index();
        let result = Value::new(next_idx);
        let _ = self.build(InstData::bitwise(Opcode::Ixor, result, arg1, arg2));
        result
    }

    /// Bitwise NOT: result = ~arg
    fn inot(self, arg: Value) -> Value
    where
        Self: Sized,
    {
        let next_idx = self.data_flow_graph().next_value_index();
        let result = Value::new(next_idx);
        let _ = self.build(InstData::bitwise_unary(Opcode::Inot, result, arg));
        result
    }

    // Shift instructions - return Value

    /// Left shift: result = arg1 << arg2
    fn ishl(self, arg1: Value, arg2: Value) -> Value
    where
        Self: Sized,
    {
        let next_idx = self.data_flow_graph().next_value_index();
        let result = Value::new(next_idx);
        let _ = self.build(InstData::shift(Opcode::Ishl, result, arg1, arg2));
        result
    }

    /// Logical right shift: result = arg1 >>> arg2 (unsigned)
    fn ishr(self, arg1: Value, arg2: Value) -> Value
    where
        Self: Sized,
    {
        let next_idx = self.data_flow_graph().next_value_index();
        let result = Value::new(next_idx);
        let _ = self.build(InstData::shift(Opcode::Ishr, result, arg1, arg2));
        result
    }

    /// Arithmetic right shift: result = arg1 >> arg2 (signed)
    fn iashr(self, arg1: Value, arg2: Value) -> Value
    where
        Self: Sized,
    {
        let next_idx = self.data_flow_graph().next_value_index();
        let result = Value::new(next_idx);
        let _ = self.build(InstData::shift(Opcode::Iashr, result, arg1, arg2));
        result
    }

    // Comparison instructions - return Value

    /// Integer comparison: result = (arg1 cond arg2)
    fn icmp(self, cond: IntCC, arg1: Value, arg2: Value) -> Value
    where
        Self: Sized,
    {
        let next_idx = self.data_flow_graph().next_value_index();
        let result = Value::new(next_idx);
        let _ = self.build(InstData::comparison(
            Opcode::Icmp { cond },
            result,
            arg1,
            arg2,
        ));
        result
    }

    /// Floating point comparison: result = (arg1 cond arg2)
    /// Note: IR-only, backend lowering not supported yet
    fn fcmp(self, cond: FloatCC, arg1: Value, arg2: Value) -> Value
    where
        Self: Sized,
    {
        let next_idx = self.data_flow_graph().next_value_index();
        let result = Value::new(next_idx);
        let _ = self.build(InstData::comparison(
            Opcode::Fcmp { cond },
            result,
            arg1,
            arg2,
        ));
        result
    }

    // Convenience methods for backward compatibility

    /// Integer compare equal: result = (arg1 == arg2)
    fn icmp_eq(self, arg1: Value, arg2: Value) -> Value
    where
        Self: Sized,
    {
        self.icmp(IntCC::Equal, arg1, arg2)
    }

    /// Integer compare not equal: result = (arg1 != arg2)
    fn icmp_ne(self, arg1: Value, arg2: Value) -> Value
    where
        Self: Sized,
    {
        self.icmp(IntCC::NotEqual, arg1, arg2)
    }

    /// Integer compare less than: result = (arg1 < arg2)
    fn icmp_lt(self, arg1: Value, arg2: Value) -> Value
    where
        Self: Sized,
    {
        self.icmp(IntCC::SignedLessThan, arg1, arg2)
    }

    /// Integer compare less than or equal: result = (arg1 <= arg2)
    fn icmp_le(self, arg1: Value, arg2: Value) -> Value
    where
        Self: Sized,
    {
        self.icmp(IntCC::SignedLessThanOrEqual, arg1, arg2)
    }

    /// Integer compare greater than: result = (arg1 > arg2)
    fn icmp_gt(self, arg1: Value, arg2: Value) -> Value
    where
        Self: Sized,
    {
        self.icmp(IntCC::SignedGreaterThan, arg1, arg2)
    }

    /// Integer compare greater than or equal: result = (arg1 >= arg2)
    fn icmp_ge(self, arg1: Value, arg2: Value) -> Value
    where
        Self: Sized,
    {
        self.icmp(IntCC::SignedGreaterThanOrEqual, arg1, arg2)
    }

    // Constant instructions - return Value

    /// Integer constant: result = value
    fn iconst(self, value: i64) -> Value
    where
        Self: Sized,
    {
        let next_idx = self.data_flow_graph().next_value_index();
        let result = Value::new(next_idx);
        let _ = self.build(InstData::constant(result, Immediate::I64(value)));
        result
    }

    /// Floating point constant: result = value
    fn fconst(self, value: f32) -> Value
    where
        Self: Sized,
    {
        let next_idx = self.data_flow_graph().next_value_index();
        let result = Value::new(next_idx);
        let _ = self.build(InstData::constant(
            result,
            Immediate::F32Bits(value.to_bits()),
        ));
        result
    }

    // Control flow instructions - return ()

    /// Jump to target block
    fn jump(self, target: Block, args: alloc::vec::Vec<Value>)
    where
        Self: Sized,
    {
        let _ = self.build(InstData::jump(target, args));
    }

    /// Conditional branch: if condition, jump to target_true, else target_false
    fn br(
        self,
        condition: Value,
        target_true: Block,
        args_true: alloc::vec::Vec<Value>,
        target_false: Block,
        args_false: alloc::vec::Vec<Value>,
    ) where
        Self: Sized,
    {
        let _ = self.build(InstData::branch(
            condition,
            target_true,
            args_true,
            target_false,
            args_false,
        ));
    }

    /// Return with values
    fn return_(self, values: alloc::vec::Vec<Value>)
    where
        Self: Sized,
    {
        let _ = self.build(InstData::return_(values));
    }

    /// Halt execution
    fn halt(self)
    where
        Self: Sized,
    {
        let _ = self.build(InstData::halt());
    }

    // Trap instructions

    /// Unconditional trap: terminate execution with trap code
    fn trap(self, code: TrapCode)
    where
        Self: Sized,
    {
        let _ = self.build(InstData::trap(code));
    }

    /// Trap if condition is zero: if condition == 0, trap with code
    fn trapz(self, condition: Value, code: TrapCode)
    where
        Self: Sized,
    {
        let _ = self.build(InstData::trapz(condition, code));
    }

    /// Trap if condition is non-zero: if condition != 0, trap with code
    fn trapnz(self, condition: Value, code: TrapCode)
    where
        Self: Sized,
    {
        let _ = self.build(InstData::trapnz(condition, code));
    }

    // Memory instructions

    /// Load from memory: result = mem[address]
    fn load(self, address: Value, ty: Type) -> Value
    where
        Self: Sized,
    {
        let next_idx = self.data_flow_graph().next_value_index();
        let result = Value::new(next_idx);
        let _ = self.build(InstData::load(result, address, ty));
        result
    }

    /// Store to memory: mem[address] = value
    fn store(self, address: Value, value: Value, ty: Type)
    where
        Self: Sized,
    {
        let _ = self.build(InstData::store(address, value, ty));
    }

    // Call instructions

    /// Function call: results = callee(args...)
    fn call(
        self,
        callee: alloc::string::String,
        args: alloc::vec::Vec<Value>,
        results: alloc::vec::Vec<Value>,
    ) -> alloc::vec::Vec<Value>
    where
        Self: Sized,
    {
        let _ = self.build(InstData::call(callee, args, results.clone()));
        results
    }

    /// System call: syscall(number, args...)
    fn syscall(self, number: i32, args: alloc::vec::Vec<Value>)
    where
        Self: Sized,
    {
        let _ = self.build(InstData::syscall(number, args, alloc::vec::Vec::new()));
    }
}

/// Blanket implementation: any type implementing InstBuilderBase gets InstBuilder methods
impl<'f, T: InstBuilderBase<'f>> InstBuilder<'f> for T {}

/// Base trait for instruction inserters.
///
/// This is an alternative base trait for an instruction builder to implement.
/// An instruction inserter can be adapted into an instruction builder by wrapping
/// it in an `InsertBuilder`.
pub trait InstInserterBase<'f>: Sized {
    /// Get an immutable reference to the data flow graph.
    fn data_flow_graph(&self) -> &DFG;

    /// Get a mutable reference to the data flow graph.
    fn data_flow_graph_mut(&mut self) -> &mut DFG;

    /// Insert a new instruction which belongs to the DFG.
    fn insert_built_inst(self, inst: InstEntity) -> &'f mut DFG;
}
