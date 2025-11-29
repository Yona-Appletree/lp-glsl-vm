//! Type checking for GLSL declarations.

use alloc::format;

use glsl::syntax::Declaration;

use crate::{
    error::{GlslError, GlslResult},
    expr::typecheck::type_check_expr,
    symbols::SymbolTable,
    types::GlslType,
    util::extract_type_from_fully_specified,
};

/// Type check a variable declaration.
pub fn type_check_declaration(symbols: &mut SymbolTable, decl: &Declaration) -> GlslResult<()> {
    match decl {
        Declaration::InitDeclaratorList(list) => {
            // Extract type from head declaration
            let ty = extract_type_from_fully_specified(&list.head.ty)
                .ok_or_else(|| GlslError::type_error("Unsupported variable type"))?;

            // Declare the head variable
            if let Some(name) = &list.head.name {
                // Check initializer if present
                if let Some(init) = &list.head.initializer {
                    let init_ty = type_check_initializer(symbols, init)?;
                    if init_ty != ty {
                        return Err(GlslError::type_error(format!(
                            "Variable '{}' type mismatch: declared as {}, initialized with {}",
                            name.0, ty, init_ty
                        )));
                    }
                }

                symbols
                    .declare_variable(name.0.clone(), ty)
                    .map_err(|e| GlslError::type_error(e))?;
            }

            // Declare tail variables
            for decl_no_type in &list.tail {
                // Tail declarations use the same type as head
                if let Some(init) = &decl_no_type.initializer {
                    let init_ty = type_check_initializer(symbols, init)?;
                    if init_ty != ty {
                        return Err(GlslError::type_error(format!(
                            "Variable type mismatch: declared as {}, initialized with {}",
                            ty, init_ty
                        )));
                    }
                }

                symbols
                    .declare_variable(decl_no_type.ident.ident.0.clone(), ty)
                    .map_err(|e| GlslError::type_error(e))?;
            }

            Ok(())
        }
        _ => Err(GlslError::type_error("Unsupported declaration type")),
    }
}

/// Type check an initializer.
fn type_check_initializer(
    symbols: &SymbolTable,
    init: &glsl::syntax::Initializer,
) -> GlslResult<GlslType> {
    match init {
        glsl::syntax::Initializer::Simple(expr) => type_check_expr(expr, symbols),
        glsl::syntax::Initializer::List(_) => {
            Err(GlslError::type_error("List initializers not supported"))
        }
    }
}

