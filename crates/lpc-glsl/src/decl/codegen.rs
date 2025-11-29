//! Code generation for GLSL declarations.

use glsl::syntax::Declaration;

use crate::{
    error::{GlslError, GlslResult},
    expr::codegen::generate_expr,
    function::codegen::CodeGenContext,
    types::GlslType,
    util::extract_type_from_fully_specified,
};

/// Generate LPIR for a variable declaration.
pub fn generate_declaration(ctx: &mut dyn CodeGenContext, decl: &Declaration) -> GlslResult<()> {
    match decl {
        Declaration::InitDeclaratorList(list) => {
            // Extract type
            let ty = extract_type_from_fully_specified(&list.head.ty)
                .ok_or_else(|| GlslError::codegen("Unsupported variable type"))?;

            // Declare head variable
            if let Some(name) = &list.head.name {
                let var_name = name.0.clone();
                let value = if let Some(init) = &list.head.initializer {
                    // Variable with initializer - extract expression from Initializer
                    match init {
                        glsl::syntax::Initializer::Simple(expr) => generate_expr(ctx, expr)?,
                        glsl::syntax::Initializer::List(_) => {
                            return Err(GlslError::codegen("List initializers not supported"));
                        }
                    }
                } else {
                    // Variable without initializer - create default value
                    create_default_value(ctx, ty)?
                };

                let block = ctx.current_block()?;
                // Record in SSABuilder
                ctx.ssa_builder_mut().record_def(&var_name, block, value);
                // Also maintain legacy tracking
                ctx.variables_mut().insert(var_name.clone(), value);
                // Track this variable in the current scope (both old and new)
                {
                    let scope_stack = ctx.scope_stack_mut();
                    if let Some(current_scope) = scope_stack.last_mut() {
                        current_scope.insert(var_name.clone());
                    }
                }
                // Also track in new scope stack
                ctx.scope_stack_new_mut().declare(var_name);
            }

            // Declare tail variables
            for decl_no_type in &list.tail {
                let var_name = decl_no_type.ident.ident.0.clone();
                let value = if let Some(init) = &decl_no_type.initializer {
                    match init {
                        glsl::syntax::Initializer::Simple(expr) => generate_expr(ctx, expr)?,
                        glsl::syntax::Initializer::List(_) => {
                            return Err(GlslError::codegen("List initializers not supported"));
                        }
                    }
                } else {
                    create_default_value(ctx, ty)?
                };

                let block = ctx.current_block()?;
                // Record in SSABuilder
                ctx.ssa_builder_mut().record_def(&var_name, block, value);
                // Also maintain legacy tracking
                ctx.variables_mut().insert(var_name.clone(), value);
                // Track this variable in the current scope (both old and new)
                {
                    let scope_stack = ctx.scope_stack_mut();
                    if let Some(current_scope) = scope_stack.last_mut() {
                        current_scope.insert(var_name.clone());
                    }
                }
                // Also track in new scope stack
                ctx.scope_stack_new_mut().declare(var_name);
            }

            Ok(())
        }
        _ => Err(GlslError::codegen("Unsupported declaration type")),
    }
}

/// Create a default value for a type.
fn create_default_value(ctx: &mut dyn CodeGenContext, ty: GlslType) -> GlslResult<lpc_lpir::Value> {
    let block = ctx.current_block()?;
    let value = ctx.builder_mut().new_value();
    let mut block_builder = ctx.builder_mut().block_builder(block);
    match ty {
        GlslType::Int | GlslType::Bool => {
            block_builder.iconst(value, 0);
        }
        GlslType::Float => {
            block_builder.fconst(value, 0.0);
        }
    }
    Ok(value)
}
