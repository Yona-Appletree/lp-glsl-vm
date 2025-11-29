//! Code generation for GLSL expressions.

use alloc::{boxed::Box, format, string::String, vec::Vec};

use glsl::syntax::Expr;
use lpc_lpir::{BlockEntity, Type, Value};

use crate::{
    error::{GlslError, GlslResult},
    function::codegen::CodeGenContext,
    symbols::SymbolTable,
    types::GlslType,
    util::extract_type_from_specifier,
};

/// Generate LPIR for an expression and return its value.
pub fn generate_expr(ctx: &mut dyn CodeGenContext, expr: &Expr) -> GlslResult<Value> {
    match expr {
        // Literals
        Expr::IntConst(i) => {
            let block = ctx.current_block()?;
            let value = ctx.builder_mut().new_value();
            let mut block_builder = ctx.builder_mut().block_builder(block);
            block_builder.iconst(value, *i as i64);
            Ok(value)
        }
        Expr::BoolConst(b) => {
            let block = ctx.current_block()?;
            let value = ctx.builder_mut().new_value();
            let mut block_builder = ctx.builder_mut().block_builder(block);
            block_builder.iconst(value, if *b { 1 } else { 0 });
            // Bool maps to u32 in LPIR, so set the type explicitly
            drop(block_builder);
            ctx.builder_mut()
                .function_mut()
                .dfg
                .set_value_type(value, Type::U32);
            Ok(value)
        }
        Expr::FloatConst(f) => {
            let block = ctx.current_block()?;
            let value = ctx.builder_mut().new_value();
            let mut block_builder = ctx.builder_mut().block_builder(block);
            block_builder.fconst(value, *f);
            Ok(value)
        }
        Expr::UIntConst(_) | Expr::DoubleConst(_) => {
            Err(GlslError::codegen("Unsupported literal type"))
        }

        // Variable reference
        Expr::Variable(ident) => {
            let name = ident.0.as_str();
            // Use lazy SSA construction to get the correct value
            let block = ctx.current_block()?;
        crate::debug!("[EXPR] Reading variable '{}' in block {:?}", name, block);
            let result = ctx.get_ssa_value(name, block)?;
            if result.is_none() {
        crate::debug!("[EXPR] Variable '{}' not found in block {:?}", name, block);
                // For 'i', get debug info
                if name == "i" {
                    let blocks: Vec<_> = {
                        use crate::codegen::SSABuilder;
                        let ssa_ptr: *const SSABuilder = ctx.ssa_builder_mut() as *const SSABuilder;
                        unsafe { (*ssa_ptr).debug_get_blocks_for_var("i") }
                    };
        crate::debug!("[EXPR] Variable 'i' available in blocks: {:?}", blocks);
                }
            } else {
        crate::debug!("[EXPR] Variable '{}' found in block {:?}, value={:?}", name, block, result);
            }
            result.ok_or_else(|| GlslError::codegen(format!("Undefined variable '{}'", name)))
        }

        // Unary operators
        Expr::Unary(op, operand) => {
            let operand_value = generate_expr(ctx, operand)?;
            generate_unary_op(ctx, op.clone(), operand_value)
        }

        // Binary operators
        Expr::Binary(op, left, right) => {
            let left_value = generate_expr(ctx, left)?;
            let right_value = generate_expr(ctx, right)?;
            generate_binary_op(ctx, op.clone(), left_value, right_value)
        }

        // Assignment
        Expr::Assignment(lhs, _op, rhs) => {
            let rhs_value = generate_expr(ctx, rhs)?;
            // Assignment can only be to a variable (type checker should have caught this)
            if let Expr::Variable(ident) = lhs.as_ref() {
                let name = ident.0.as_str();
                let block = ctx.current_block()?;
                // Record definition in SSABuilder
                ctx.ssa_builder_mut().record_def(name, block, rhs_value);
                // Also maintain legacy tracking for backward compatibility
                ctx.variables_mut().insert(String::from(name), rhs_value);
                Ok(rhs_value)
            } else {
                Err(GlslError::codegen(
                    "Assignment can only be to a variable, not to an expression",
                ))
            }
        }

        // Function call
        Expr::FunCall(fun_ident, args) => generate_function_call(ctx, fun_ident, args),

        // Not supported
        Expr::Ternary(_, _, _) => Err(GlslError::codegen("Ternary operator not supported")),
        Expr::Bracket(_, _) => Err(GlslError::codegen("Array indexing not supported")),
        Expr::Dot(_, _) => Err(GlslError::codegen("Struct field access not supported")),
        Expr::PostInc(_) | Expr::PostDec(_) => {
            Err(GlslError::codegen("Post-increment/decrement not supported"))
        }
        Expr::Comma(_, _) => Err(GlslError::codegen("Comma operator not supported")),
    }
}

/// Generate LPIR for a function call expression.
fn generate_function_call(
    ctx: &mut dyn CodeGenContext,
    fun_ident: &glsl::syntax::FunIdentifier,
    args: &[glsl::syntax::Expr],
) -> GlslResult<Value> {
    let name = match fun_ident {
        glsl::syntax::FunIdentifier::Identifier(ident) => ident.0.as_str(),
        _ => {
            return Err(GlslError::codegen(
                "Complex function identifiers not supported",
            ))
        }
    };

    // Get function signature to determine parameter types and qualifiers
    let sig = ctx
        .symbols()
        .lookup_function(name)
        .ok_or_else(|| GlslError::codegen(format!("Undefined function '{}'", name)))?;

    if args.len() != sig.params.len() {
        return Err(GlslError::codegen(format!(
            "Function '{}' expects {} arguments, got {}",
            name,
            sig.params.len(),
            args.len()
        )));
    }

    // Extract parameter info before borrowing ctx mutably
    let param_info: Vec<(bool, bool, GlslType)> = sig
        .params
        .iter()
        .map(|p| {
            (
                p.qualifier.is_by_reference(),
                p.qualifier == crate::symbols::ParameterQualifier::InOut,
                p.ty,
            )
        })
        .collect();
    let return_type = sig.return_type;

    let block = ctx.current_block()?;

    // Generate argument values, handling out/inout parameters
    let mut arg_values = Vec::new();
    let mut out_inout_info: Vec<(Value, GlslType, Option<String>)> = Vec::new(); // (address, type, variable_name) for out/inout params

    for (arg_expr, (is_by_ref, is_inout, param_type)) in args.iter().zip(param_info.iter()) {
        if *is_by_ref {
            // For out/inout: allocate stack space and pass address
            let arg_value = generate_expr(ctx, arg_expr)?;
            let lpir_type = param_type.to_lpir();

            // Track variable name if argument is a variable
            let var_name = if let Expr::Variable(ident) = arg_expr {
                Some(ident.0.clone())
            } else {
                None
            };

            // Allocate stack space for the parameter
            let address_value = ctx.builder_mut().new_value();
            let size = param_type.size_in_bytes();
            let mut block_builder = ctx.builder_mut().block_builder(block);
            block_builder.stackalloc(address_value, size);
            drop(block_builder);
            ctx.builder_mut()
                .function_mut()
                .dfg
                .set_value_type(address_value, Type::I32);

            // For inout: store the current value to address before call
            if *is_inout {
                let mut block_builder = ctx.builder_mut().block_builder(block);
                block_builder.store(address_value, arg_value, lpir_type);
                drop(block_builder);
            }
            // For out: storage is uninitialized (will be written by callee)

            arg_values.push(address_value);
            out_inout_info.push((address_value, *param_type, var_name));
        } else {
            // For in: pass by value
            let arg_value = generate_expr(ctx, arg_expr)?;
            arg_values.push(arg_value);
        }
    }

    // Generate return value(s)
    let mut return_values = Vec::new();
    if return_type.is_some() {
        let return_value = ctx.builder_mut().new_value();
        return_values.push(return_value);
    }

    // Generate call instruction
    let mut block_builder = ctx.builder_mut().block_builder(block);
    block_builder.call(String::from(name), arg_values, return_values.clone());
    drop(block_builder);

    // After call: load results from out/inout parameters and update variables
    if !out_inout_info.is_empty() {
        // Create values first, then get block builder
        let mut loaded_values = Vec::new();
        for (address_value, param_type, var_name) in &out_inout_info {
            let loaded_value = ctx.builder_mut().new_value();
            loaded_values.push((
                loaded_value,
                *address_value,
                param_type.to_lpir(),
                var_name.clone(),
            ));
        }

        // Now generate load instructions and update variables
        let mut block_builder = ctx.builder_mut().block_builder(block);
        let mut var_updates = Vec::new();
        for (loaded_value, address_value, lpir_type, var_name) in loaded_values {
            block_builder.load(loaded_value, address_value, lpir_type);
            // Collect variable updates to apply after dropping block_builder
            if let Some(name) = var_name {
                var_updates.push((name, loaded_value));
            }
        }
        drop(block_builder);
        // Apply variable updates
        let block = ctx.current_block()?;
        for (name, value) in var_updates {
            // Record in SSABuilder
            ctx.ssa_builder_mut().record_def(&name, block, value);
            // Also maintain legacy tracking
            ctx.variables_mut().insert(name, value);
        }
    }

    // Return the first return value (or error if void and value is required)
    // Note: Void function calls are allowed in expression statements,
    // but generate_expr always expects a value, so we error here
    // The caller (expression statement handler) will catch this and allow it
    return_values
        .first()
        .copied()
        .ok_or_else(|| GlslError::void_function_call(name))
}

/// Generate LPIR for a unary operator.
pub fn generate_unary_op(
    ctx: &mut dyn CodeGenContext,
    op: glsl::syntax::UnaryOp,
    operand: Value,
) -> GlslResult<Value> {
    let block = ctx.current_block()?;
    let result = ctx.builder_mut().new_value();
    let zero = ctx.builder_mut().new_value();
    let mut block_builder = ctx.builder_mut().block_builder(block);

    match op {
        glsl::syntax::UnaryOp::Minus => {
            // Negate: result = 0 - operand
            block_builder.iconst(zero, 0);
            block_builder.isub(result, zero, operand);
            Ok(result)
        }
        glsl::syntax::UnaryOp::Not => {
            // Logical not: result = operand == 0 ? 1 : 0
            // Use icmp_eq to compare with zero
            block_builder.iconst(zero, 0);
            // Set zero to u32 type to match operand (bool is u32)
            drop(block_builder);
            ctx.builder_mut()
                .function_mut()
                .dfg
                .set_value_type(zero, Type::U32);
            let mut block_builder = ctx.builder_mut().block_builder(block);
            block_builder.icmp_eq(result, operand, zero);
            // Bool maps to u32 in LPIR
            drop(block_builder);
            ctx.builder_mut()
                .function_mut()
                .dfg
                .set_value_type(result, Type::U32);
            Ok(result)
        }
        _ => Err(GlslError::codegen("Unsupported unary operator")),
    }
}

/// Generate LPIR for a binary operator.
pub fn generate_binary_op(
    ctx: &mut dyn CodeGenContext,
    op: glsl::syntax::BinaryOp,
    left: Value,
    right: Value,
) -> GlslResult<Value> {
    let block = ctx.current_block()?;
    let result = ctx.builder_mut().new_value();

    // Check operand types
    let left_ty = ctx.builder_mut().function().dfg.value_type(left);
    let right_ty = ctx.builder_mut().function().dfg.value_type(right);

    let is_float_op = left_ty == Some(Type::F32) && right_ty == Some(Type::F32);

    let mut block_builder = ctx.builder_mut().block_builder(block);

    match op {
        glsl::syntax::BinaryOp::Add => {
            if is_float_op {
                block_builder.fadd(result, left, right);
            } else {
                block_builder.iadd(result, left, right);
            }
            Ok(result)
        }
        glsl::syntax::BinaryOp::Sub => {
            if is_float_op {
                block_builder.fsub(result, left, right);
            } else {
                block_builder.isub(result, left, right);
            }
            Ok(result)
        }
        glsl::syntax::BinaryOp::Mult => {
            if is_float_op {
                block_builder.fmul(result, left, right);
            } else {
                block_builder.imul(result, left, right);
            }
            Ok(result)
        }
        glsl::syntax::BinaryOp::Div => {
            if is_float_op {
                block_builder.fdiv(result, left, right);
            } else {
                block_builder.idiv(result, left, right);
            }
            Ok(result)
        }
        glsl::syntax::BinaryOp::Mod => {
            block_builder.irem(result, left, right);
            Ok(result)
        }
        glsl::syntax::BinaryOp::LT => {
            if is_float_op {
                use lpc_lpir::FloatCC;
                block_builder.fcmp(result, FloatCC::LessThan, left, right);
            } else {
                block_builder.icmp_lt(result, left, right);
            }
            // Bool maps to u32 in LPIR
            drop(block_builder);
            ctx.builder_mut()
                .function_mut()
                .dfg
                .set_value_type(result, Type::U32);
            Ok(result)
        }
        glsl::syntax::BinaryOp::GT => {
            if is_float_op {
                use lpc_lpir::FloatCC;
                block_builder.fcmp(result, FloatCC::GreaterThan, left, right);
            } else {
                block_builder.icmp_gt(result, left, right);
            }
            // Bool maps to u32 in LPIR
            drop(block_builder);
            ctx.builder_mut()
                .function_mut()
                .dfg
                .set_value_type(result, Type::U32);
            Ok(result)
        }
        glsl::syntax::BinaryOp::LTE => {
            if is_float_op {
                use lpc_lpir::FloatCC;
                block_builder.fcmp(result, FloatCC::LessThanOrEqual, left, right);
            } else {
                block_builder.icmp_le(result, left, right);
            }
            // Bool maps to u32 in LPIR
            drop(block_builder);
            ctx.builder_mut()
                .function_mut()
                .dfg
                .set_value_type(result, Type::U32);
            Ok(result)
        }
        glsl::syntax::BinaryOp::GTE => {
            if is_float_op {
                use lpc_lpir::FloatCC;
                block_builder.fcmp(result, FloatCC::GreaterThanOrEqual, left, right);
            } else {
                block_builder.icmp_ge(result, left, right);
            }
            // Bool maps to u32 in LPIR
            drop(block_builder);
            ctx.builder_mut()
                .function_mut()
                .dfg
                .set_value_type(result, Type::U32);
            Ok(result)
        }
        glsl::syntax::BinaryOp::Equal => {
            if is_float_op {
                use lpc_lpir::FloatCC;
                block_builder.fcmp(result, FloatCC::Equal, left, right);
            } else {
                block_builder.icmp_eq(result, left, right);
            }
            // Bool maps to u32 in LPIR
            drop(block_builder);
            ctx.builder_mut()
                .function_mut()
                .dfg
                .set_value_type(result, Type::U32);
            Ok(result)
        }
        glsl::syntax::BinaryOp::NonEqual => {
            if is_float_op {
                use lpc_lpir::FloatCC;
                block_builder.fcmp(result, FloatCC::NotEqual, left, right);
            } else {
                block_builder.icmp_ne(result, left, right);
            }
            // Bool maps to u32 in LPIR
            drop(block_builder);
            ctx.builder_mut()
                .function_mut()
                .dfg
                .set_value_type(result, Type::U32);
            Ok(result)
        }
        glsl::syntax::BinaryOp::And => {
            // Logical AND: both must be non-zero
            // Since bool is u32 (0 or 1), bitwise AND works perfectly for logical AND
            block_builder.iand(result, left, right);
            // Bool maps to u32 in LPIR
            drop(block_builder);
            ctx.builder_mut()
                .function_mut()
                .dfg
                .set_value_type(result, Type::U32);
            Ok(result)
        }
        glsl::syntax::BinaryOp::Or => {
            // Logical OR: at least one must be non-zero
            // Since bool is u32 (0 or 1), bitwise OR works perfectly for logical OR
            block_builder.ior(result, left, right);
            // Bool maps to u32 in LPIR
            drop(block_builder);
            ctx.builder_mut()
                .function_mut()
                .dfg
                .set_value_type(result, Type::U32);
            Ok(result)
        }
        _ => Err(GlslError::codegen("Unsupported binary operator")),
    }
}
