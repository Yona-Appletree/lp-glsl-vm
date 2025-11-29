//! Value representation for GLSL codegen.
//!
//! This module provides abstractions for representing lvalues and rvalues
//! in GLSL code generation, similar to Clang's CGValue system.
//!
//! **Note**: These types are currently unused but will be migrated to in the future.
//! See `docs/glsl/05-values.md` for the migration plan.
//!
//! TODO: Migrate codebase to use GlslValue/GlslLValue/GlslRValue types
//! (see docs/glsl/05-values.md)

#![allow(dead_code)]

use lpc_lpir::{Type, Value};

use crate::types::GlslType;

/// Trait for operations needed by value loading/storing.
/// This avoids circular dependencies.
///
/// TODO: Migrate codebase to use this trait (see docs/glsl/05-values.md)
#[allow(dead_code)]
pub trait ValueBuilder {
    fn load(&mut self, result: Value, address: Value, ty: Type);
    fn store(&mut self, address: Value, value: Value, ty: Type);
    fn new_value(&mut self) -> Value;
}

/// Represents an rvalue - the result of evaluating an expression.
///
/// TODO: Migrate codebase to use this type (see docs/glsl/05-values.md)
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub enum GlslRValue {
    /// Simple scalar value (int, float, bool)
    Scalar(Value),
    /// Address of an aggregate (for structs/arrays - future)
    Aggregate(Value),
    // Future: Complex, Vector, etc.
}

/// Represents an lvalue - something that can be assigned to.
///
/// TODO: Migrate codebase to use this type (see docs/glsl/05-values.md)
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct GlslLValue {
    /// Address of the value
    address: Value,
    /// Type of the value
    ty: GlslType,
    /// Alignment requirement
    alignment: u32,
    /// Whether this is a reference parameter
    is_reference: bool,
}

/// A value that can be either an lvalue or rvalue.
///
/// TODO: Migrate codebase to use this type (see docs/glsl/05-values.md)
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum GlslValue {
    LValue(GlslLValue),
    RValue(GlslRValue),
}

impl GlslRValue {
    /// Convert to SSA value (for scalar) or get address (for aggregate).
    pub fn to_value(&self) -> Value {
        match self {
            GlslRValue::Scalar(v) => *v,
            GlslRValue::Aggregate(v) => *v,
        }
    }

    /// Load from memory if aggregate.
    ///
    /// For scalars, this is a no-op. For aggregates, this loads the value
    /// from the address.
    ///
    /// Requires a new value to be created from FunctionBuilder before calling.
    pub fn load<B: ValueBuilder>(self, builder: &mut B, value: Value, ty: GlslType) -> GlslRValue {
        match self {
            GlslRValue::Scalar(v) => GlslRValue::Scalar(v),
            GlslRValue::Aggregate(addr) => {
                let lpir_type = ty.to_lpir();
                builder.load(value, addr, lpir_type);
                GlslRValue::Scalar(value)
            }
        }
    }
}

impl GlslLValue {
    /// Create a new lvalue from an address.
    pub fn new(address: Value, ty: GlslType, alignment: u32, is_reference: bool) -> Self {
        Self {
            address,
            ty,
            alignment,
            is_reference,
        }
    }

    /// Get the address of this lvalue.
    pub fn address(&self) -> Value {
        self.address
    }

    /// Get the type of this lvalue.
    pub fn ty(&self) -> GlslType {
        self.ty
    }

    /// Store an rvalue into this lvalue.
    pub fn store<B: ValueBuilder>(self, value: GlslRValue, builder: &mut B) {
        let lpir_type = self.ty.to_lpir();
        let value_to_store = value.to_value();
        builder.store(self.address, value_to_store, lpir_type);
    }

    /// Load the value from this lvalue.
    ///
    /// Requires a new value to be created from FunctionBuilder before calling.
    pub fn load<B: ValueBuilder>(self, builder: &mut B, value: Value) -> GlslRValue {
        let lpir_type = self.ty.to_lpir();
        builder.load(value, self.address, lpir_type);
        GlslRValue::Scalar(value)
    }

    /// Convert this lvalue to an rvalue by loading.
    ///
    /// Requires a new value to be created from FunctionBuilder before calling.
    pub fn to_rvalue<B: ValueBuilder>(self, builder: &mut B, value: Value) -> GlslRValue {
        self.load(builder, value)
    }
}

impl GlslValue {
    /// Create an lvalue.
    pub fn lvalue(lvalue: GlslLValue) -> Self {
        Self::LValue(lvalue)
    }

    /// Create an rvalue.
    pub fn rvalue(rvalue: GlslRValue) -> Self {
        Self::RValue(rvalue)
    }

    /// Convert to an rvalue, loading if necessary.
    ///
    /// Requires a new value to be created from FunctionBuilder before calling if this is an lvalue.
    pub fn to_rvalue<B: ValueBuilder>(self, builder: &mut B, value: Value) -> GlslRValue {
        match self {
            GlslValue::LValue(lval) => lval.to_rvalue(builder, value),
            GlslValue::RValue(rval) => rval,
        }
    }

    /// Get the value as a scalar value, loading if necessary.
    ///
    /// Requires a new value to be created from FunctionBuilder before calling if this is an lvalue.
    pub fn to_value<B: ValueBuilder>(self, builder: &mut B, value: Value) -> Value {
        self.to_rvalue(builder, value).to_value()
    }
}
