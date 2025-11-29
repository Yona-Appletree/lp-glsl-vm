//! CodeGen builder wrapper.
//!
//! This module provides a wrapper that adds metadata and provides
//! a cleaner API for code generation.

use alloc::{string::String, vec::Vec};

use lpc_lpir::{BlockEntity, FunctionBuilder, Type, Value};

use crate::codegen::value::ValueBuilder;

/// Wrapper that provides a cleaner API for code generation.
///
/// This provides a centralized place for adding metadata, debug info,
/// and other codegen-related functionality.
pub struct CodeGenBuilder<'a> {
    block: BlockEntity,
    function_builder: &'a mut FunctionBuilder,
}

impl<'a> CodeGenBuilder<'a> {
    /// Create a new CodeGenBuilder.
    pub fn new(block: BlockEntity, function_builder: &'a mut FunctionBuilder) -> Self {
        Self {
            block,
            function_builder,
        }
    }

    /// Get a reference to the FunctionBuilder.
    pub fn function_builder(&mut self) -> &mut FunctionBuilder {
        self.function_builder
    }

    /// Get the block this builder is for.
    pub fn block(&self) -> BlockEntity {
        self.block
    }

    /// Create a new value.
    pub fn new_value(&mut self) -> Value {
        self.function_builder.new_value()
    }

    // Delegate operations to BlockBuilder (created on-demand)

    /// Integer constant.
    pub fn iconst(&mut self, value: Value, imm: i64) {
        self.function_builder
            .block_builder(self.block)
            .iconst(value, imm);
    }

    /// Integer add.
    pub fn iadd(&mut self, result: Value, lhs: Value, rhs: Value) {
        self.function_builder
            .block_builder(self.block)
            .iadd(result, lhs, rhs);
    }

    /// Integer subtract.
    pub fn isub(&mut self, result: Value, lhs: Value, rhs: Value) {
        self.function_builder
            .block_builder(self.block)
            .isub(result, lhs, rhs);
    }

    /// Integer multiply.
    pub fn imul(&mut self, result: Value, lhs: Value, rhs: Value) {
        self.function_builder
            .block_builder(self.block)
            .imul(result, lhs, rhs);
    }

    /// Integer divide.
    pub fn idiv(&mut self, result: Value, lhs: Value, rhs: Value) {
        self.function_builder
            .block_builder(self.block)
            .idiv(result, lhs, rhs);
    }

    /// Integer remainder.
    pub fn irem(&mut self, result: Value, lhs: Value, rhs: Value) {
        self.function_builder
            .block_builder(self.block)
            .irem(result, lhs, rhs);
    }

    /// Integer bitwise AND.
    pub fn iand(&mut self, result: Value, lhs: Value, rhs: Value) {
        self.function_builder
            .block_builder(self.block)
            .iand(result, lhs, rhs);
    }

    /// Integer bitwise OR.
    pub fn ior(&mut self, result: Value, lhs: Value, rhs: Value) {
        self.function_builder
            .block_builder(self.block)
            .ior(result, lhs, rhs);
    }

    /// Integer compare less than.
    pub fn icmp_lt(&mut self, result: Value, lhs: Value, rhs: Value) {
        self.function_builder
            .block_builder(self.block)
            .icmp_lt(result, lhs, rhs);
    }

    /// Integer compare greater than.
    pub fn icmp_gt(&mut self, result: Value, lhs: Value, rhs: Value) {
        self.function_builder
            .block_builder(self.block)
            .icmp_gt(result, lhs, rhs);
    }

    /// Integer compare less than or equal.
    pub fn icmp_le(&mut self, result: Value, lhs: Value, rhs: Value) {
        self.function_builder
            .block_builder(self.block)
            .icmp_le(result, lhs, rhs);
    }

    /// Integer compare greater than or equal.
    pub fn icmp_ge(&mut self, result: Value, lhs: Value, rhs: Value) {
        self.function_builder
            .block_builder(self.block)
            .icmp_ge(result, lhs, rhs);
    }

    /// Integer compare equal.
    pub fn icmp_eq(&mut self, result: Value, lhs: Value, rhs: Value) {
        self.function_builder
            .block_builder(self.block)
            .icmp_eq(result, lhs, rhs);
    }

    /// Integer compare not equal.
    pub fn icmp_ne(&mut self, result: Value, lhs: Value, rhs: Value) {
        self.function_builder
            .block_builder(self.block)
            .icmp_ne(result, lhs, rhs);
    }

    /// Branch instruction.
    pub fn br(
        &mut self,
        cond: Value,
        target_true: BlockEntity,
        args_true: &[Value],
        target_false: BlockEntity,
        args_false: &[Value],
    ) {
        self.function_builder.block_builder(self.block).br(
            cond,
            target_true,
            args_true,
            target_false,
            args_false,
        );
    }

    /// Jump instruction.
    pub fn jump(&mut self, target: BlockEntity, args: &[Value]) {
        self.function_builder
            .block_builder(self.block)
            .jump(target, args);
    }

    /// Return instruction.
    pub fn return_(&mut self, values: &[Value]) {
        self.function_builder
            .block_builder(self.block)
            .return_(values);
    }

    /// Stack allocation.
    pub fn stackalloc(&mut self, result: Value, size: u32) {
        self.function_builder
            .block_builder(self.block)
            .stackalloc(result, size);
    }

    /// Load from memory.
    pub fn load(&mut self, result: Value, address: Value, ty: Type) {
        self.function_builder
            .block_builder(self.block)
            .load(result, address, ty);
    }

    /// Store to memory.
    pub fn store(&mut self, address: Value, value: Value, ty: Type) {
        self.function_builder
            .block_builder(self.block)
            .store(address, value, ty);
    }

    /// Function call.
    pub fn call(&mut self, callee: String, args: Vec<Value>, results: Vec<Value>) {
        self.function_builder
            .block_builder(self.block)
            .call(callee, args, results);
    }
}

impl<'a> ValueBuilder for CodeGenBuilder<'a> {
    fn load(&mut self, result: Value, address: Value, ty: Type) {
        self.load(result, address, ty);
    }

    fn store(&mut self, address: Value, value: Value, ty: Type) {
        self.store(address, value, ty);
    }

    fn new_value(&mut self) -> Value {
        self.new_value()
    }
}
