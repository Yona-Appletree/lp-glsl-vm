//! Code generation: Convert type-checked GLSL AST to LPIR.
//!
//! This module generates LPIR (Low-level Program Intermediate Representation)
//! from the type-checked GLSL AST.

use alloc::{boxed::Box, format, string::String, vec, vec::Vec};

use glsl::syntax::{
    CompoundStatement, Declaration, Expr, ForInitStatement, FunctionDefinition, IterationStatement,
    JumpStatement, SelectionRestStatement, SelectionStatement, SimpleStatement, Statement,
};
use lpc_lpir::{BlockEntity, Function, FunctionBuilder, Opcode, Signature, Type, Value};

use crate::{
    error::{GlslError, GlslResult},
    symbols::SymbolTable,
    types::GlslType,
};

/// Code generator context.
///
/// This holds the function builder and tracks SSA values for variables.
pub struct CodeGen {
    /// Function builder for constructing LPIR
    builder: FunctionBuilder,
    /// Current block being built
    current_block: Option<BlockEntity>,
    /// Variable name to SSA value mapping (for current scope)
    variables: alloc::collections::BTreeMap<String, Value>,
    /// Symbol table for function lookups
    symbols: SymbolTable,
    /// Scope stack: each entry is a set of variable names declared in that scope
    scope_stack: Vec<alloc::collections::BTreeSet<String>>,
    /// Out/inout parameter tracking: variable name -> (address_param, type)
    out_inout_params: alloc::collections::BTreeMap<String, (Value, GlslType)>,
}

impl CodeGen {
    /// Create a new code generator for a function.
    pub fn new(name: String, signature: Signature) -> Self {
        let builder = FunctionBuilder::new(signature, name);
        Self {
            builder,
            current_block: None,
            variables: alloc::collections::BTreeMap::new(),
            symbols: SymbolTable::new(),
            scope_stack: Vec::new(),
            out_inout_params: alloc::collections::BTreeMap::new(),
        }
    }

    /// Get the current block, returning an error if none is set.
    fn current_block(&self) -> GlslResult<BlockEntity> {
        self.current_block
            .ok_or_else(|| GlslError::codegen("Internal error: no current block set"))
    }

    /// Finish building and return the function.
    pub fn finish(self) -> Function {
        self.builder.finish()
    }

    /// Generate LPIR for a function definition.
    pub fn generate_function(
        &mut self,
        func_def: &FunctionDefinition,
        symbols: &SymbolTable,
    ) -> GlslResult<()> {
        // Store symbol table for function lookups
        self.symbols = symbols.clone();

        // Extract function signature to get parameter qualifiers
        let func_sig = crate::typecheck::TypeChecker::extract_function_signature(func_def)?;

        // Create entry block with parameters
        let mut entry_params = Vec::new();
        let mut param_types = Vec::new();

        for (idx, param_decl) in func_def.prototype.parameters.iter().enumerate() {
            let param_idx = entry_params.len() as u32;
            let param_value = Value::new(param_idx);
            entry_params.push(param_value);

            // Extract parameter type and qualifier
            if let glsl::syntax::FunctionParameterDeclaration::Named(_, declarator) = param_decl {
                let glsl_type = Self::extract_type_from_specifier(&declarator.ty)
                    .ok_or_else(|| GlslError::codegen("Unsupported parameter type"))?;

                let param_qualifier = func_sig.params[idx].qualifier;
                if param_qualifier.is_by_reference() {
                    // For out/inout: parameter is an address (I32)
                    param_types.push(Type::I32);
                } else {
                    // For in: parameter is the value type
                    let lpir_type = glsl_type.to_lpir();
                    param_types.push(lpir_type);
                }
            }
        }

        let entry_block = if entry_params.is_empty() {
            self.builder.create_block()
        } else {
            self.builder.block_with_params(entry_params.clone())
        };

        // Set parameter types in block data
        if !entry_params.is_empty() {
            if let Some(block_data) = self.builder.function_mut().block_data_mut(entry_block) {
                block_data.param_types = param_types.clone();
            }

            // Set value types in DFG for parameters
            for (param_value, param_type) in entry_params.iter().zip(param_types.iter()) {
                self.builder
                    .function_mut()
                    .dfg
                    .set_value_type(*param_value, *param_type);
            }
        }

        self.current_block = Some(entry_block);

        // Advance SSA counter past parameters
        for _ in 0..entry_params.len() {
            let _ = self.builder.new_value();
        }

        // Add parameters to variable map, handling out/inout
        for (idx, param_decl) in func_def.prototype.parameters.iter().enumerate() {
            if let glsl::syntax::FunctionParameterDeclaration::Named(_, declarator) = param_decl {
                let name = declarator.ident.ident.0.clone();
                let param_qualifier = func_sig.params[idx].qualifier;

                if param_qualifier.is_by_reference() {
                    // For out/inout: parameter is an address (I32)
                    let address_param = entry_params[idx];
                    let glsl_type = func_sig.params[idx].ty;
                    let lpir_type = glsl_type.to_lpir();

                    let loaded_value =
                        if param_qualifier == crate::symbols::ParameterQualifier::InOut {
                            // For inout: load the initial value from address
                            let value = self.builder.new_value();
                            let mut block_builder = self.builder.block_builder(entry_block);
                            block_builder.load(value, address_param, lpir_type);
                            drop(block_builder);
                            value
                        } else {
                            // For out: create default value (will be overwritten before return)
                            // The variable starts with a default value, but the callee will write to the address
                            self.create_default_value(glsl_type)?
                        };

                    // Store mapping: variable name -> (address, type) for storing back before return
                    self.out_inout_params
                        .insert(name.clone(), (address_param, glsl_type));
                    self.variables.insert(name, loaded_value);
                } else {
                    // For in: parameter is the value itself
                    self.variables.insert(name, entry_params[idx]);
                }
            }
        }

        // Generate code for function body
        let body_stmt = Statement::Compound(Box::new(func_def.statement.clone()));
        self.generate_statement(&body_stmt)?;

        // Check if function ends with a return statement
        // If not, add an implicit return (required for LPIR)
        let current_block = self.current_block()?;
        let ends_with_return = self.block_ends_with_return_or_halt(current_block);

        if !ends_with_return {
            // Function doesn't end with return - add implicit return
            // First, store out/inout parameters if any
            if !self.out_inout_params.is_empty() {
                let mut block_builder = self.builder.block_builder(current_block);
                for (var_name, (address_param, glsl_type)) in &self.out_inout_params {
                    if let Some(current_value) = self.variables.get(var_name) {
                        let lpir_type = glsl_type.to_lpir();
                        block_builder.store(*address_param, *current_value, lpir_type);
                    }
                }
                drop(block_builder);
            }

            // Add implicit return
            let mut block_builder = self.builder.block_builder(current_block);
            block_builder.return_(&Vec::new());
        }

        Ok(())
    }

    /// Generate LPIR for a statement.
    fn generate_statement(&mut self, stmt: &Statement) -> GlslResult<()> {
        match stmt {
            Statement::Simple(simple) => self.generate_simple_statement(simple),
            Statement::Compound(compound) => self.generate_compound_statement(compound),
        }
    }

    /// Generate LPIR for a compound statement (block).
    fn generate_compound_statement(&mut self, compound: &CompoundStatement) -> GlslResult<()> {
        // Push new scope
        self.scope_stack.push(alloc::collections::BTreeSet::new());

        // Generate each statement
        for stmt in &compound.statement_list {
            self.generate_statement(stmt)?;
        }

        // Pop scope: remove only variables declared in this scope
        if let Some(scope_vars) = self.scope_stack.pop() {
            for var_name in scope_vars {
                self.variables.remove(&var_name);
            }
        }

        Ok(())
    }

    /// Generate LPIR for a simple statement.
    fn generate_simple_statement(&mut self, simple: &SimpleStatement) -> GlslResult<()> {
        match simple {
            SimpleStatement::Declaration(decl) => {
                self.generate_declaration(decl)?;
                Ok(())
            }
            SimpleStatement::Expression(expr_stmt) => {
                if let Some(expr) = expr_stmt {
                    // Generate expression - void function calls are allowed here
                    // They don't return a value, but that's OK for expression statements
                    match self.generate_expr(expr) {
                        Ok(_) => {
                            // Expression has a value - that's fine
                        }
                        Err(e) => {
                            // Check if it's a void function call error - if so, allow it
                            if matches!(&e, GlslError::VoidFunctionCall(_)) {
                                // Void function call - this is allowed in expression statements
                                // The call was already generated, we just don't use the return value
                            } else {
                                return Err(e);
                            }
                        }
                    }
                }
                Ok(())
            }
            SimpleStatement::Selection(sel) => self.generate_selection_statement(sel),
            SimpleStatement::Iteration(iter) => self.generate_iteration_statement(iter),
            SimpleStatement::Jump(jump) => self.generate_jump_statement(jump),
            SimpleStatement::Switch(_) => Err(GlslError::codegen("Switch not supported")),
            SimpleStatement::CaseLabel(_) => Err(GlslError::codegen("Case labels not supported")),
        }
    }

    /// Generate LPIR for a variable declaration.
    fn generate_declaration(&mut self, decl: &Declaration) -> GlslResult<()> {
        match decl {
            Declaration::InitDeclaratorList(list) => {
                // Extract type
                let ty = Self::extract_type_from_fully_specified(&list.head.ty)
                    .ok_or_else(|| GlslError::codegen("Unsupported variable type"))?;

                // Declare head variable
                if let Some(name) = &list.head.name {
                    let var_name = name.0.clone();
                    let value = if let Some(init) = &list.head.initializer {
                        // Variable with initializer - extract expression from Initializer
                        match init {
                            glsl::syntax::Initializer::Simple(expr) => self.generate_expr(expr)?,
                            glsl::syntax::Initializer::List(_) => {
                                return Err(GlslError::codegen("List initializers not supported"));
                            }
                        }
                    } else {
                        // Variable without initializer - create default value
                        self.create_default_value(ty)?
                    };

                    self.variables.insert(var_name.clone(), value);
                    // Track this variable in the current scope
                    if let Some(current_scope) = self.scope_stack.last_mut() {
                        current_scope.insert(var_name);
                    }
                }

                // Declare tail variables
                for decl_no_type in &list.tail {
                    let var_name = decl_no_type.ident.ident.0.clone();
                    let value = if let Some(init) = &decl_no_type.initializer {
                        match init {
                            glsl::syntax::Initializer::Simple(expr) => self.generate_expr(expr)?,
                            glsl::syntax::Initializer::List(_) => {
                                return Err(GlslError::codegen("List initializers not supported"));
                            }
                        }
                    } else {
                        self.create_default_value(ty)?
                    };

                    self.variables.insert(var_name.clone(), value);
                    // Track this variable in the current scope
                    if let Some(current_scope) = self.scope_stack.last_mut() {
                        current_scope.insert(var_name);
                    }
                }

                Ok(())
            }
            _ => Err(GlslError::codegen("Unsupported declaration type")),
        }
    }

    /// Create a default value for a type.
    fn create_default_value(&mut self, ty: GlslType) -> GlslResult<Value> {
        let block = self.current_block()?;
        let value = self.builder.new_value();
        let mut block_builder = self.builder.block_builder(block);
        match ty {
            GlslType::Int | GlslType::Bool => {
                block_builder.iconst(value, 0);
            }
        }
        Ok(value)
    }

    /// Generate LPIR for an expression and return its value.
    fn generate_expr(&mut self, expr: &Expr) -> GlslResult<Value> {
        match expr {
            // Literals
            Expr::IntConst(i) => {
                let block = self.current_block()?;
                let value = self.builder.new_value();
                let mut block_builder = self.builder.block_builder(block);
                block_builder.iconst(value, *i as i64);
                Ok(value)
            }
            Expr::BoolConst(b) => {
                let block = self.current_block()?;
                let value = self.builder.new_value();
                let mut block_builder = self.builder.block_builder(block);
                block_builder.iconst(value, if *b { 1 } else { 0 });
                // Bool maps to u32 in LPIR, so set the type explicitly
                drop(block_builder);
                self.builder
                    .function_mut()
                    .dfg
                    .set_value_type(value, Type::U32);
                Ok(value)
            }
            Expr::UIntConst(_) | Expr::FloatConst(_) | Expr::DoubleConst(_) => {
                Err(GlslError::codegen("Unsupported literal type"))
            }

            // Variable reference
            Expr::Variable(ident) => {
                let name = ident.0.as_str();
                self.variables
                    .get(name)
                    .copied()
                    .ok_or_else(|| GlslError::codegen(format!("Undefined variable '{}'", name)))
            }

            // Unary operators
            Expr::Unary(op, operand) => {
                let operand_value = self.generate_expr(operand)?;
                self.generate_unary_op(op.clone(), operand_value)
            }

            // Binary operators
            Expr::Binary(op, left, right) => {
                let left_value = self.generate_expr(left)?;
                let right_value = self.generate_expr(right)?;
                self.generate_binary_op(op.clone(), left_value, right_value)
            }

            // Assignment
            Expr::Assignment(lhs, _op, rhs) => {
                let rhs_value = self.generate_expr(rhs)?;
                // Assignment can only be to a variable (type checker should have caught this)
                if let Expr::Variable(ident) = lhs.as_ref() {
                    let name = ident.0.clone();
                    self.variables.insert(name, rhs_value);
                    Ok(rhs_value)
                } else {
                    Err(GlslError::codegen(
                        "Assignment can only be to a variable, not to an expression",
                    ))
                }
            }

            // Function call
            Expr::FunCall(fun_ident, args) => {
                let name = match fun_ident {
                    glsl::syntax::FunIdentifier::Identifier(ident) => ident.0.as_str(),
                    _ => {
                        return Err(GlslError::codegen(
                            "Complex function identifiers not supported",
                        ))
                    }
                };

                // Get function signature to determine parameter types and qualifiers
                let sig = self
                    .symbols
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

                // Extract parameter info before borrowing self mutably
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

                let block = self.current_block()?;

                // Generate argument values, handling out/inout parameters
                let mut arg_values = Vec::new();
                let mut out_inout_info: Vec<(Value, GlslType, Option<String>)> = Vec::new(); // (address, type, variable_name) for out/inout params

                for (arg_expr, (is_by_ref, is_inout, param_type)) in
                    args.iter().zip(param_info.iter())
                {
                    if *is_by_ref {
                        // For out/inout: allocate stack space and pass address
                        let arg_value = self.generate_expr(arg_expr)?;
                        let lpir_type = param_type.to_lpir();

                        // Track variable name if argument is a variable
                        let var_name = if let Expr::Variable(ident) = arg_expr {
                            Some(ident.0.clone())
                        } else {
                            None
                        };

                        // Allocate stack space for the parameter
                        let address_value = self.builder.new_value();
                        let size = param_type.size_in_bytes();
                        let mut block_builder = self.builder.block_builder(block);
                        block_builder.stackalloc(address_value, size);
                        drop(block_builder);
                        self.builder
                            .function_mut()
                            .dfg
                            .set_value_type(address_value, Type::I32);

                        // For inout: store the current value to address before call
                        if *is_inout {
                            let mut block_builder = self.builder.block_builder(block);
                            block_builder.store(address_value, arg_value, lpir_type);
                            drop(block_builder);
                        }
                        // For out: storage is uninitialized (will be written by callee)

                        arg_values.push(address_value);
                        out_inout_info.push((address_value, *param_type, var_name));
                    } else {
                        // For in: pass by value
                        let arg_value = self.generate_expr(arg_expr)?;
                        arg_values.push(arg_value);
                    }
                }

                // Generate return value(s)
                let mut return_values = Vec::new();
                if return_type.is_some() {
                    let return_value = self.builder.new_value();
                    return_values.push(return_value);
                }

                // Generate call instruction
                let mut block_builder = self.builder.block_builder(block);
                block_builder.call(String::from(name), arg_values, return_values.clone());
                drop(block_builder);

                // After call: load results from out/inout parameters and update variables
                if !out_inout_info.is_empty() {
                    // Create values first, then get block builder
                    let mut loaded_values = Vec::new();
                    for (address_value, param_type, var_name) in &out_inout_info {
                        let loaded_value = self.builder.new_value();
                        loaded_values.push((
                            loaded_value,
                            *address_value,
                            param_type.to_lpir(),
                            var_name.clone(),
                        ));
                    }

                    // Now generate load instructions and update variables
                    let mut block_builder = self.builder.block_builder(block);
                    for (loaded_value, address_value, lpir_type, var_name) in loaded_values {
                        block_builder.load(loaded_value, address_value, lpir_type);
                        // Assign loaded_value back to the original variable if it was a variable
                        if let Some(name) = var_name {
                            self.variables.insert(name, loaded_value);
                        }
                    }
                    drop(block_builder);
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

    /// Generate LPIR for a unary operator.
    fn generate_unary_op(
        &mut self,
        op: glsl::syntax::UnaryOp,
        operand: Value,
    ) -> GlslResult<Value> {
        let block = self.current_block()?;
        let result = self.builder.new_value();
        let zero = self.builder.new_value();
        let mut block_builder = self.builder.block_builder(block);

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
                self.builder
                    .function_mut()
                    .dfg
                    .set_value_type(zero, Type::U32);
                let mut block_builder = self.builder.block_builder(block);
                block_builder.icmp_eq(result, operand, zero);
                // Bool maps to u32 in LPIR
                drop(block_builder);
                self.builder
                    .function_mut()
                    .dfg
                    .set_value_type(result, Type::U32);
                Ok(result)
            }
            _ => Err(GlslError::codegen("Unsupported unary operator")),
        }
    }

    /// Generate LPIR for a binary operator.
    fn generate_binary_op(
        &mut self,
        op: glsl::syntax::BinaryOp,
        left: Value,
        right: Value,
    ) -> GlslResult<Value> {
        let block = self.current_block()?;
        let result = self.builder.new_value();
        let mut block_builder = self.builder.block_builder(block);

        match op {
            glsl::syntax::BinaryOp::Add => {
                block_builder.iadd(result, left, right);
                Ok(result)
            }
            glsl::syntax::BinaryOp::Sub => {
                block_builder.isub(result, left, right);
                Ok(result)
            }
            glsl::syntax::BinaryOp::Mult => {
                block_builder.imul(result, left, right);
                Ok(result)
            }
            glsl::syntax::BinaryOp::Div => {
                block_builder.idiv(result, left, right);
                Ok(result)
            }
            glsl::syntax::BinaryOp::Mod => {
                block_builder.irem(result, left, right);
                Ok(result)
            }
            glsl::syntax::BinaryOp::LT => {
                block_builder.icmp_lt(result, left, right);
                // Bool maps to u32 in LPIR
                drop(block_builder);
                self.builder
                    .function_mut()
                    .dfg
                    .set_value_type(result, Type::U32);
                Ok(result)
            }
            glsl::syntax::BinaryOp::GT => {
                block_builder.icmp_gt(result, left, right);
                // Bool maps to u32 in LPIR
                drop(block_builder);
                self.builder
                    .function_mut()
                    .dfg
                    .set_value_type(result, Type::U32);
                Ok(result)
            }
            glsl::syntax::BinaryOp::LTE => {
                block_builder.icmp_le(result, left, right);
                // Bool maps to u32 in LPIR
                drop(block_builder);
                self.builder
                    .function_mut()
                    .dfg
                    .set_value_type(result, Type::U32);
                Ok(result)
            }
            glsl::syntax::BinaryOp::GTE => {
                block_builder.icmp_ge(result, left, right);
                // Bool maps to u32 in LPIR
                drop(block_builder);
                self.builder
                    .function_mut()
                    .dfg
                    .set_value_type(result, Type::U32);
                Ok(result)
            }
            glsl::syntax::BinaryOp::Equal => {
                block_builder.icmp_eq(result, left, right);
                // Bool maps to u32 in LPIR
                drop(block_builder);
                self.builder
                    .function_mut()
                    .dfg
                    .set_value_type(result, Type::U32);
                Ok(result)
            }
            glsl::syntax::BinaryOp::NonEqual => {
                block_builder.icmp_ne(result, left, right);
                // Bool maps to u32 in LPIR
                drop(block_builder);
                self.builder
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
                self.builder
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
                self.builder
                    .function_mut()
                    .dfg
                    .set_value_type(result, Type::U32);
                Ok(result)
            }
            _ => Err(GlslError::codegen("Unsupported binary operator")),
        }
    }

    /// Generate LPIR for a selection statement (if/else).
    fn generate_selection_statement(&mut self, sel: &SelectionStatement) -> GlslResult<()> {
        // Save variable state before the if statement
        let pre_if_vars = self.variables.clone();

        // Generate condition first (before getting block builder)
        let cond_value = self.generate_expr(&sel.cond)?;

        // Create blocks for true and false branches
        let block = self.current_block()?;
        let true_block = self.builder.create_block();
        let false_block = self.builder.create_block();

        // Now get block builder for branch instruction
        let mut block_builder = self.builder.block_builder(block);
        block_builder.br(
            cond_value,
            true_block,
            &Vec::new(),
            false_block,
            &Vec::new(),
        );

        // Generate true branch
        self.current_block = Some(true_block);
        match &sel.rest {
            SelectionRestStatement::Statement(true_stmt) => {
                self.generate_statement(true_stmt)?;
                // Check what block we ended up in after generating the statement
                let true_end_block = self.current_block()?;
                let true_ends_with_return_or_halt =
                    self.block_ends_with_return_or_halt(true_end_block);

                // Save variable state after true branch
                let true_end_vars = self.variables.clone();

                // Restore pre-if state for false branch
                self.variables = pre_if_vars.clone();

                // False branch is empty - no changes to variables
                let false_end_vars = pre_if_vars.clone();

                // Find variables that were modified in at least one branch
                let mut modified_vars = alloc::collections::BTreeSet::new();
                for var_name in true_end_vars.keys() {
                    if let (Some(true_val), Some(false_val)) =
                        (true_end_vars.get(var_name), false_end_vars.get(var_name))
                    {
                        // Variable exists in both - check if values differ
                        if true_val != false_val {
                            modified_vars.insert(var_name.clone());
                        }
                    } else if true_end_vars.contains_key(var_name) {
                        // Variable exists only in true branch (was modified)
                        modified_vars.insert(var_name.clone());
                    }
                }
                for var_name in false_end_vars.keys() {
                    if !true_end_vars.contains_key(var_name) {
                        // Variable exists only in false branch (was modified)
                        modified_vars.insert(var_name.clone());
                    }
                }

                // Create phi nodes for modified variables
                let mut phi_params = Vec::new();
                let mut phi_param_types = Vec::new();
                let mut var_to_phi_idx: alloc::collections::BTreeMap<String, usize> =
                    alloc::collections::BTreeMap::new();
                let mut phi_var_names = Vec::new();

                for var_name in &modified_vars {
                    // Get the type from the pre-if value or true branch value
                    let var_type = if let Some(val) = pre_if_vars.get(var_name) {
                        self.builder
                            .function_mut()
                            .dfg
                            .value_type(*val)
                            .unwrap_or(Type::I32)
                    } else if let Some(val) = true_end_vars.get(var_name) {
                        self.builder
                            .function_mut()
                            .dfg
                            .value_type(*val)
                            .unwrap_or(Type::I32)
                    } else {
                        Type::I32
                    };

                    let phi_param = self.builder.new_value();
                    phi_params.push(phi_param);
                    phi_param_types.push(var_type);
                    let idx = phi_var_names.len();
                    phi_var_names.push(var_name.clone());
                    var_to_phi_idx.insert(var_name.clone(), idx);

                    // Set phi parameter type
                    self.builder
                        .function_mut()
                        .dfg
                        .set_value_type(phi_param, var_type);
                }

                // Create merge block with phi parameters if needed
                let merge_block = if phi_params.is_empty() {
                    self.builder.create_block()
                } else {
                    self.builder.block_with_params(phi_params.clone())
                };

                // Set phi parameter types in block data
                if !phi_params.is_empty() {
                    if let Some(block_data) =
                        self.builder.function_mut().block_data_mut(merge_block)
                    {
                        block_data.param_types = phi_param_types.clone();
                    }
                }

                if !true_ends_with_return_or_halt {
                    // Collect values from true branch for phi nodes - use current variable state
                    // at the point where we jump
                    let mut true_values = Vec::new();
                    for var_name in &phi_var_names {
                        // Use the value from true_end_vars (the state after generating true branch)
                        if let Some(val) = true_end_vars.get(var_name) {
                            true_values.push(*val);
                        } else if let Some(val) = pre_if_vars.get(var_name) {
                            // Variable wasn't modified in true branch, use pre-if value
                            true_values.push(*val);
                        } else {
                            return Err(GlslError::codegen(format!(
                                "Variable '{}' not found for phi node",
                                var_name
                            )));
                        }
                    }

                    // Need to jump to merge block from wherever we ended up
                    if true_end_block != true_block {
                        // Statement ended in a different block (e.g., for loop exit) - jump from there
                        // Restore variable state to what it was at true_end_block
                        self.variables = true_end_vars.clone();
                        let mut end_block_builder = self.builder.block_builder(true_end_block);
                        if phi_params.is_empty() {
                            end_block_builder.jump(merge_block, &Vec::new());
                        } else {
                            end_block_builder.jump(merge_block, &true_values);
                        }
                    } else {
                        // Statement ended in true_block - jump from there
                        // Restore variable state to what it was at true_block
                        self.variables = true_end_vars.clone();
                        let mut true_block_builder = self.builder.block_builder(true_block);
                        if phi_params.is_empty() {
                            true_block_builder.jump(merge_block, &Vec::new());
                        } else {
                            true_block_builder.jump(merge_block, &true_values);
                        }
                    }
                }

                // Collect values from false branch for phi nodes - use pre-if state
                // since false branch didn't modify anything
                let mut false_values = Vec::new();
                for var_name in &phi_var_names {
                    if let Some(val) = pre_if_vars.get(var_name) {
                        false_values.push(*val);
                    } else {
                        return Err(GlslError::codegen(format!(
                            "Variable '{}' not found for phi node",
                            var_name
                        )));
                    }
                }

                // False branch is empty - jump directly to merge block
                // Restore variable state to pre-if state for false branch
                self.variables = pre_if_vars.clone();
                let mut false_block_builder = self.builder.block_builder(false_block);
                if phi_params.is_empty() {
                    false_block_builder.jump(merge_block, &Vec::new());
                } else {
                    false_block_builder.jump(merge_block, &false_values);
                }

                // Update variables map to use phi parameters
                for (var_name, phi_idx) in &var_to_phi_idx {
                    self.variables
                        .insert(var_name.clone(), phi_params[*phi_idx]);
                }

                // Continue in merge block
                self.current_block = Some(merge_block);
            }
            SelectionRestStatement::Else(true_stmt, false_stmt) => {
                self.generate_statement(true_stmt)?;
                // Check what block we ended up in after generating the statement
                let true_end_block = self.current_block()?;
                let true_ends_with_return_or_halt =
                    self.block_ends_with_return_or_halt(true_end_block);

                // Save variable state after true branch
                let true_end_vars = self.variables.clone();

                // Restore pre-if state for false branch
                self.variables = pre_if_vars.clone();

                // Generate false branch
                self.current_block = Some(false_block);
                self.generate_statement(false_stmt)?;
                // Check what block we ended up in after generating the statement
                let false_end_block = self.current_block()?;
                let false_ends_with_return_or_halt =
                    self.block_ends_with_return_or_halt(false_end_block);

                // Save variable state after false branch
                let false_end_vars = self.variables.clone();

                // Find variables that were modified in at least one branch
                let mut modified_vars = alloc::collections::BTreeSet::new();
                for var_name in true_end_vars.keys() {
                    if let (Some(true_val), Some(false_val)) =
                        (true_end_vars.get(var_name), false_end_vars.get(var_name))
                    {
                        // Variable exists in both - check if values differ
                        if true_val != false_val {
                            modified_vars.insert(var_name.clone());
                        }
                    } else if true_end_vars.contains_key(var_name) {
                        // Variable exists only in true branch (was modified)
                        modified_vars.insert(var_name.clone());
                    }
                }
                for var_name in false_end_vars.keys() {
                    if !true_end_vars.contains_key(var_name) {
                        // Variable exists only in false branch (was modified)
                        modified_vars.insert(var_name.clone());
                    }
                }

                // Only create merge block if at least one branch doesn't return/halt
                if !true_ends_with_return_or_halt || !false_ends_with_return_or_halt {
                    // Create phi nodes for modified variables
                    let mut phi_params = Vec::new();
                    let mut phi_param_types = Vec::new();
                    let mut var_to_phi_idx: alloc::collections::BTreeMap<String, usize> =
                        alloc::collections::BTreeMap::new();
                    let mut phi_var_names = Vec::new();

                    for var_name in &modified_vars {
                        // Get the type from the pre-if value or true branch value
                        let var_type = if let Some(val) = pre_if_vars.get(var_name) {
                            self.builder
                                .function_mut()
                                .dfg
                                .value_type(*val)
                                .unwrap_or(Type::I32)
                        } else if let Some(val) = true_end_vars.get(var_name) {
                            self.builder
                                .function_mut()
                                .dfg
                                .value_type(*val)
                                .unwrap_or(Type::I32)
                        } else {
                            Type::I32
                        };

                        let phi_param = self.builder.new_value();
                        phi_params.push(phi_param);
                        phi_param_types.push(var_type);
                        let idx = phi_var_names.len();
                        phi_var_names.push(var_name.clone());
                        var_to_phi_idx.insert(var_name.clone(), idx);

                        // Set phi parameter type
                        self.builder
                            .function_mut()
                            .dfg
                            .set_value_type(phi_param, var_type);
                    }

                    // Create merge block with phi parameters if needed
                    let merge_block = if phi_params.is_empty() {
                        self.builder.create_block()
                    } else {
                        self.builder.block_with_params(phi_params.clone())
                    };

                    // Set phi parameter types in block data
                    if !phi_params.is_empty() {
                        if let Some(block_data) =
                            self.builder.function_mut().block_data_mut(merge_block)
                        {
                            block_data.param_types = phi_param_types.clone();
                        }
                    }

                    if !true_ends_with_return_or_halt {
                        // Collect values from true branch for phi nodes
                        let mut true_values = Vec::new();
                        for var_name in &phi_var_names {
                            if let Some(val) = true_end_vars.get(var_name) {
                                true_values.push(*val);
                            } else if let Some(val) = pre_if_vars.get(var_name) {
                                // Variable wasn't modified in true branch, use pre-if value
                                true_values.push(*val);
                            } else {
                                return Err(GlslError::codegen(format!(
                                    "Variable '{}' not found for phi node",
                                    var_name
                                )));
                            }
                        }

                        // Need to jump to merge block from wherever we ended up
                        if true_end_block != true_block {
                            // Statement ended in a different block (e.g., merge block from nested if) - jump from there
                            // Restore variable state to what it was at true_end_block
                            self.variables = true_end_vars.clone();
                            let mut end_block_builder = self.builder.block_builder(true_end_block);
                            if phi_params.is_empty() {
                                end_block_builder.jump(merge_block, &Vec::new());
                            } else {
                                end_block_builder.jump(merge_block, &true_values);
                            }
                        } else {
                            // Statement ended in true_block - jump from there
                            // Restore variable state to what it was at true_block
                            self.variables = true_end_vars.clone();
                            let mut true_block_builder = self.builder.block_builder(true_block);
                            if phi_params.is_empty() {
                                true_block_builder.jump(merge_block, &Vec::new());
                            } else {
                                true_block_builder.jump(merge_block, &true_values);
                            }
                        }
                    }

                    if !false_ends_with_return_or_halt {
                        // Collect values from false branch for phi nodes
                        let mut false_values = Vec::new();
                        for var_name in &phi_var_names {
                            if let Some(val) = false_end_vars.get(var_name) {
                                false_values.push(*val);
                            } else if let Some(val) = pre_if_vars.get(var_name) {
                                // Variable wasn't modified in false branch, use pre-if value
                                false_values.push(*val);
                            } else {
                                return Err(GlslError::codegen(format!(
                                    "Variable '{}' not found for phi node",
                                    var_name
                                )));
                            }
                        }

                        // Need to jump to merge block from wherever we ended up
                        if false_end_block != false_block {
                            // Statement ended in a different block - jump from there
                            // Restore variable state to what it was at false_end_block
                            self.variables = false_end_vars.clone();
                            let mut end_block_builder = self.builder.block_builder(false_end_block);
                            if phi_params.is_empty() {
                                end_block_builder.jump(merge_block, &Vec::new());
                            } else {
                                end_block_builder.jump(merge_block, &false_values);
                            }
                        } else {
                            // Statement ended in false_block - jump from there
                            // Restore variable state to what it was at false_block
                            self.variables = false_end_vars.clone();
                            let mut false_block_builder = self.builder.block_builder(false_block);
                            if phi_params.is_empty() {
                                false_block_builder.jump(merge_block, &Vec::new());
                            } else {
                                false_block_builder.jump(merge_block, &false_values);
                            }
                        }
                    }

                    // Update variables map to use phi parameters
                    for (var_name, phi_idx) in &var_to_phi_idx {
                        self.variables
                            .insert(var_name.clone(), phi_params[*phi_idx]);
                    }

                    // Continue in merge block
                    self.current_block = Some(merge_block);
                }
                // If both branches return/halt, we don't create a merge block and current_block
                // is left pointing to the false_end_block (which has return/halt)
            }
        };

        Ok(())
    }

    /// Check if a block ends with a return or halt instruction (i.e., is unreachable).
    fn block_ends_with_return_or_halt(&mut self, block: BlockEntity) -> bool {
        let func = self.builder.function_mut();
        let insts: Vec<_> = func.block_insts(block).collect();
        if let Some(last_inst) = insts.last() {
            if let Some(inst_data) = func.dfg.inst_data(*last_inst) {
                matches!(inst_data.opcode, Opcode::Return | Opcode::Halt)
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Generate LPIR for an iteration statement (for/while).
    fn generate_iteration_statement(&mut self, iter: &IterationStatement) -> GlslResult<()> {
        match iter {
            IterationStatement::While(cond, body) => {
                // Save variable state before loop
                let pre_loop_vars = self.variables.clone();

                // Find variables referenced in the condition
                let cond_vars = match cond {
                    glsl::syntax::Condition::Expr(expr) => Self::find_variable_references(expr),
                    glsl::syntax::Condition::Assignment(_, _, _) => {
                        return Err(GlslError::codegen(
                            "Assignment in while condition not supported",
                        ))
                    }
                };

                // Create phi nodes for variables used in condition
                let mut phi_params = Vec::new();
                let mut phi_param_types = Vec::new();
                let mut var_to_phi_idx: alloc::collections::BTreeMap<String, usize> =
                    alloc::collections::BTreeMap::new();
                let mut phi_var_names = Vec::new();

                for var_name in &cond_vars {
                    if let Some(initial_val) = pre_loop_vars.get(var_name) {
                        // Get the type of the initial value
                        let var_type = self
                            .builder
                            .function_mut()
                            .dfg
                            .value_type(*initial_val)
                            .unwrap_or(Type::I32); // Default to I32 if not set

                        let phi_param = self.builder.new_value();
                        phi_params.push(phi_param);
                        phi_param_types.push(var_type);
                        let idx = phi_var_names.len();
                        phi_var_names.push(var_name.clone());
                        var_to_phi_idx.insert(var_name.clone(), idx);

                        // Set phi parameter type
                        self.builder
                            .function_mut()
                            .dfg
                            .set_value_type(phi_param, var_type);
                    }
                }

                // Create loop header block with phi parameters
                let entry_block = self.current_block()?;
                let loop_header = if phi_params.is_empty() {
                    self.builder.create_block()
                } else {
                    self.builder.block_with_params(phi_params.clone())
                };

                // Set phi parameter types in block data
                if !phi_params.is_empty() {
                    if let Some(block_data) =
                        self.builder.function_mut().block_data_mut(loop_header)
                    {
                        block_data.param_types = phi_param_types.clone();
                    }
                }

                // Update variables map to use phi parameters
                for (var_name, phi_idx) in &var_to_phi_idx {
                    self.variables
                        .insert(var_name.clone(), phi_params[*phi_idx]);
                }

                // Jump from entry to loop header with initial values
                let mut entry_builder = self.builder.block_builder(entry_block);
                let mut initial_values = Vec::new();
                for var_name in &phi_var_names {
                    if let Some(initial_val) = pre_loop_vars.get(var_name) {
                        initial_values.push(*initial_val);
                    }
                }
                if phi_params.is_empty() {
                    entry_builder.jump(loop_header, &Vec::new());
                } else {
                    entry_builder.jump(loop_header, &initial_values);
                }

                // Generate condition in loop header
                self.current_block = Some(loop_header);
                let body_block = self.builder.create_block();
                let exit_block = self.builder.create_block();

                let cond_value = match cond {
                    glsl::syntax::Condition::Expr(expr) => self.generate_expr(expr)?,
                    _ => unreachable!(),
                };

                // Branch: if condition, go to body, else exit
                let mut loop_header_builder = self.builder.block_builder(loop_header);
                loop_header_builder.br(
                    cond_value,
                    body_block,
                    &Vec::new(),
                    exit_block,
                    &Vec::new(),
                );

                // Generate body
                self.current_block = Some(body_block);
                self.generate_statement(body)?;

                // Check what block we ended up in after generating the body
                let body_end_block = self.current_block()?;

                // Collect updated values for phi nodes (in same order as phi_var_names)
                let mut updated_values = Vec::new();
                for var_name in &phi_var_names {
                    if let Some(updated_val) = self.variables.get(var_name) {
                        updated_values.push(*updated_val);
                    } else {
                        // Variable was removed, use initial value
                        if let Some(initial_val) = pre_loop_vars.get(var_name) {
                            updated_values.push(*initial_val);
                        }
                    }
                }

                // Jump back to loop header with updated values
                if body_end_block != body_block {
                    let mut end_block_builder = self.builder.block_builder(body_end_block);
                    if phi_params.is_empty() {
                        end_block_builder.jump(loop_header, &Vec::new());
                    } else {
                        end_block_builder.jump(loop_header, &updated_values);
                    }
                } else {
                    let mut body_block_builder = self.builder.block_builder(body_block);
                    if phi_params.is_empty() {
                        body_block_builder.jump(loop_header, &Vec::new());
                    } else {
                        body_block_builder.jump(loop_header, &updated_values);
                    }
                }

                // After loop, variables should use the phi node results (which are already in variables map)
                // Continue in exit block
                self.current_block = Some(exit_block);

                Ok(())
            }
            IterationStatement::DoWhile(body, cond_expr) => {
                // Save variable state before loop
                let pre_loop_vars = self.variables.clone();

                // Find variables referenced in the condition
                let cond_vars = Self::find_variable_references(cond_expr);

                // Create phi nodes for variables used in condition
                let mut phi_params = Vec::new();
                let mut phi_param_types = Vec::new();
                let mut var_to_phi_idx: alloc::collections::BTreeMap<String, usize> =
                    alloc::collections::BTreeMap::new();
                let mut phi_var_names = Vec::new();

                for var_name in &cond_vars {
                    if let Some(initial_val) = pre_loop_vars.get(var_name) {
                        // Get the type of the initial value
                        let var_type = self
                            .builder
                            .function_mut()
                            .dfg
                            .value_type(*initial_val)
                            .unwrap_or(Type::I32);

                        let phi_param = self.builder.new_value();
                        phi_params.push(phi_param);
                        phi_param_types.push(var_type);
                        let idx = phi_var_names.len();
                        phi_var_names.push(var_name.clone());
                        var_to_phi_idx.insert(var_name.clone(), idx);

                        // Set phi parameter type
                        self.builder
                            .function_mut()
                            .dfg
                            .set_value_type(phi_param, var_type);
                    }
                }

                // Create body block with phi parameters (loop header)
                let entry_block = self.current_block()?;
                let body_block = if phi_params.is_empty() {
                    self.builder.create_block()
                } else {
                    self.builder.block_with_params(phi_params.clone())
                };

                // Set phi parameter types in block data
                if !phi_params.is_empty() {
                    if let Some(block_data) = self.builder.function_mut().block_data_mut(body_block)
                    {
                        block_data.param_types = phi_param_types.clone();
                    }
                }

                // Update variables map to use phi parameters
                for (var_name, phi_idx) in &var_to_phi_idx {
                    self.variables
                        .insert(var_name.clone(), phi_params[*phi_idx]);
                }

                // Collect initial values for phi nodes
                let mut initial_values = Vec::new();
                for var_name in &phi_var_names {
                    if let Some(initial_val) = pre_loop_vars.get(var_name) {
                        initial_values.push(*initial_val);
                    }
                }

                // Jump from entry to body with initial values
                let mut entry_builder = self.builder.block_builder(entry_block);
                if phi_params.is_empty() {
                    entry_builder.jump(body_block, &Vec::new());
                } else {
                    entry_builder.jump(body_block, &initial_values);
                }

                // Generate body
                self.current_block = Some(body_block);
                self.generate_statement(body)?;

                // Check what block we ended up in after generating the body
                let body_end_block = self.current_block.expect("No current block after body");

                // Collect updated values for phi nodes
                let mut updated_values = Vec::new();
                for var_name in &phi_var_names {
                    if let Some(updated_val) = self.variables.get(var_name) {
                        updated_values.push(*updated_val);
                    } else {
                        // Variable was removed, use initial value
                        if let Some(initial_val) = pre_loop_vars.get(var_name) {
                            updated_values.push(*initial_val);
                        }
                    }
                }

                // Create condition and exit blocks
                let cond_block = self.builder.create_block();
                let exit_block = self.builder.create_block();

                // Jump to condition with updated values
                if body_end_block != body_block {
                    let mut end_block_builder = self.builder.block_builder(body_end_block);
                    if phi_params.is_empty() {
                        end_block_builder.jump(cond_block, &Vec::new());
                    } else {
                        end_block_builder.jump(cond_block, &updated_values);
                    }
                } else {
                    let mut body_block_builder = self.builder.block_builder(body_block);
                    if phi_params.is_empty() {
                        body_block_builder.jump(cond_block, &Vec::new());
                    } else {
                        body_block_builder.jump(cond_block, &updated_values);
                    }
                }

                // Generate condition
                self.current_block = Some(cond_block);
                let cond_value = self.generate_expr(cond_expr)?;

                // Branch: if condition, go to body, else exit
                // Note: when jumping back to body, we need to pass updated values for phi nodes
                let mut cond_builder = self.builder.block_builder(cond_block);
                if phi_params.is_empty() {
                    cond_builder.br(cond_value, body_block, &Vec::new(), exit_block, &Vec::new());
                } else {
                    // For the true branch, pass updated values (they're already computed above)
                    // For the false branch, pass empty (we're exiting)
                    cond_builder.br(
                        cond_value,
                        body_block,
                        &updated_values,
                        exit_block,
                        &Vec::new(),
                    );
                }

                // Continue in exit block
                self.current_block = Some(exit_block);

                Ok(())
            }
            IterationStatement::For(init, rest, body) => {
                // Save variable state before loop
                let pre_loop_vars = self.variables.clone();

                // Find variables referenced in the condition
                let cond_vars = if let Some(cond) = &rest.condition {
                    match cond {
                        glsl::syntax::Condition::Expr(expr) => Self::find_variable_references(expr),
                        glsl::syntax::Condition::Assignment(_, _, _) => {
                            return Err(GlslError::codegen(
                                "Assignment in for condition not supported",
                            ))
                        }
                    }
                } else {
                    alloc::collections::BTreeSet::new()
                };

                // Generate initialization
                let entry_block = self.current_block()?;
                match init {
                    ForInitStatement::Expression(expr_opt) => {
                        if let Some(expr) = expr_opt {
                            self.generate_expr(expr)?;
                        }
                    }
                    ForInitStatement::Declaration(decl) => {
                        self.generate_declaration(decl)?;
                    }
                }

                // Find variables referenced in the body (for phi nodes)
                // We need to find variables that might be modified in the body
                // For now, create phi nodes for all variables that exist before the loop
                // This is conservative but correct - variables that aren't modified will
                // just have the same value in all iterations
                let body_vars = Self::find_variable_references_in_statement(body);
                let mut loop_vars = cond_vars.clone();
                loop_vars.extend(body_vars);

                // Create phi nodes for variables used in condition or body
                let mut phi_params = Vec::new();
                let mut phi_param_types = Vec::new();
                let mut var_to_phi_idx: alloc::collections::BTreeMap<String, usize> =
                    alloc::collections::BTreeMap::new();
                let mut phi_var_names = Vec::new();

                for var_name in &loop_vars {
                    if let Some(initial_val) = self.variables.get(var_name) {
                        // Get the type of the initial value
                        let var_type = self
                            .builder
                            .function_mut()
                            .dfg
                            .value_type(*initial_val)
                            .unwrap_or(Type::I32);

                        let phi_param = self.builder.new_value();
                        phi_params.push(phi_param);
                        phi_param_types.push(var_type);
                        let idx = phi_var_names.len();
                        phi_var_names.push(var_name.clone());
                        var_to_phi_idx.insert(var_name.clone(), idx);

                        // Set phi parameter type
                        self.builder
                            .function_mut()
                            .dfg
                            .set_value_type(phi_param, var_type);
                    }
                }

                // Create condition block with phi parameters (loop header)
                let cond_block = if phi_params.is_empty() {
                    self.builder.create_block()
                } else {
                    self.builder.block_with_params(phi_params.clone())
                };

                // Set phi parameter types in block data
                if !phi_params.is_empty() {
                    if let Some(block_data) = self.builder.function_mut().block_data_mut(cond_block)
                    {
                        block_data.param_types = phi_param_types.clone();
                    }
                }

                // Update variables map to use phi parameters
                for (var_name, phi_idx) in &var_to_phi_idx {
                    self.variables
                        .insert(var_name.clone(), phi_params[*phi_idx]);
                }

                // Collect initial values for phi nodes
                let mut initial_values = Vec::new();
                for var_name in &phi_var_names {
                    if let Some(initial_val) = pre_loop_vars.get(var_name) {
                        initial_values.push(*initial_val);
                    }
                }

                // Jump from entry to condition with initial values
                let mut entry_builder = self.builder.block_builder(entry_block);
                if phi_params.is_empty() {
                    entry_builder.jump(cond_block, &Vec::new());
                } else {
                    entry_builder.jump(cond_block, &initial_values);
                }

                // Generate condition
                self.current_block = Some(cond_block);
                let body_block = self.builder.create_block();
                let inc_block = self.builder.create_block();

                // Check if condition exists - if not, exit_block is unreachable
                let has_condition = rest.condition.is_some();
                let exit_block = if has_condition {
                    Some(self.builder.create_block())
                } else {
                    None
                };

                let cond_value = if let Some(cond) = &rest.condition {
                    match cond {
                        glsl::syntax::Condition::Expr(expr) => self.generate_expr(expr)?,
                        _ => unreachable!(),
                    }
                } else {
                    // No condition means always true
                    let true_val = self.builder.new_value();
                    let mut cond_builder = self.builder.block_builder(cond_block);
                    cond_builder.iconst(true_val, 1);
                    drop(cond_builder);
                    self.builder
                        .function_mut()
                        .dfg
                        .set_value_type(true_val, Type::U32);
                    true_val
                };

                let mut cond_builder = self.builder.block_builder(cond_block);
                // Branch: if condition, go to body, else exit (if exit exists)
                if let Some(exit) = exit_block {
                    cond_builder.br(cond_value, body_block, &Vec::new(), exit, &Vec::new());
                } else {
                    // No exit block - always jump to body (condition is always true)
                    cond_builder.jump(body_block, &Vec::new());
                }

                // Generate body
                self.current_block = Some(body_block);
                self.generate_statement(body)?;

                // If body ended in a different block (e.g., merge block from nested if),
                // we need to jump from that block to increment
                let body_end_block = self.current_block()?;
                if body_end_block != body_block {
                    // Body ended in a merge block - jump to increment
                    let mut merge_builder = self.builder.block_builder(body_end_block);
                    merge_builder.jump(inc_block, &Vec::new());
                } else {
                    // Body ended in body_block - jump to increment
                    let mut body_block_builder = self.builder.block_builder(body_block);
                    body_block_builder.jump(inc_block, &Vec::new());
                }

                // Generate increment
                self.current_block = Some(inc_block);
                if let Some(post_expr) = &rest.post_expr {
                    self.generate_expr(post_expr)?;
                }

                // Collect updated values for phi nodes (in same order as phi_var_names)
                let mut updated_values = Vec::new();
                for var_name in &phi_var_names {
                    if let Some(updated_val) = self.variables.get(var_name) {
                        updated_values.push(*updated_val);
                    } else {
                        // Variable was removed, use initial value
                        if let Some(initial_val) = pre_loop_vars.get(var_name) {
                            updated_values.push(*initial_val);
                        }
                    }
                }

                // Jump back to condition with updated values
                let mut inc_builder = self.builder.block_builder(inc_block);
                if phi_params.is_empty() {
                    inc_builder.jump(cond_block, &Vec::new());
                } else {
                    inc_builder.jump(cond_block, &updated_values);
                }

                // Exit block will be continued by the next statement
                if let Some(exit) = exit_block {
                    self.current_block = Some(exit);
                }

                Ok(())
            }
        }
    }

    /// Generate LPIR for a jump statement (return/break/continue).
    fn generate_jump_statement(&mut self, jump: &JumpStatement) -> GlslResult<()> {
        let block = self.current_block()?;

        match jump {
            JumpStatement::Return(expr_opt) => {
                // Before returning, store out/inout parameters back to their addresses
                if !self.out_inout_params.is_empty() {
                    let mut block_builder = self.builder.block_builder(block);
                    for (var_name, (address_param, glsl_type)) in &self.out_inout_params {
                        if let Some(current_value) = self.variables.get(var_name) {
                            let lpir_type = glsl_type.to_lpir();
                            block_builder.store(*address_param, *current_value, lpir_type);
                        }
                    }
                    drop(block_builder);
                }

                if let Some(expr) = expr_opt {
                    // Generate expression first (creates and drops its own block builder)
                    let return_value = self.generate_expr(expr)?;
                    // Now get block builder for return instruction
                    let mut block_builder = self.builder.block_builder(block);
                    block_builder.return_(&vec![return_value]);
                } else {
                    let mut block_builder = self.builder.block_builder(block);
                    block_builder.return_(&Vec::new());
                }
                Ok(())
            }
            JumpStatement::Break | JumpStatement::Continue => {
                Err(GlslError::codegen("Break/continue not supported"))
            }
            JumpStatement::Discard => Err(GlslError::codegen("Discard not supported")),
        }
    }

    /// Extract type from fully specified type (helper method).
    fn extract_type_from_fully_specified(
        ty: &glsl::syntax::FullySpecifiedType,
    ) -> Option<GlslType> {
        GlslType::from_glsl_type_specifier(&ty.ty.ty)
    }

    /// Extract type from type specifier (helper method).
    fn extract_type_from_specifier(ty: &glsl::syntax::TypeSpecifier) -> Option<GlslType> {
        GlslType::from_glsl_type_specifier(&ty.ty)
    }

    /// Find all variable names referenced in an expression.
    fn find_variable_references(expr: &Expr) -> alloc::collections::BTreeSet<String> {
        let mut vars = alloc::collections::BTreeSet::new();
        match expr {
            Expr::Variable(ident) => {
                vars.insert(ident.0.clone());
            }
            Expr::Unary(_, operand) => {
                vars.extend(Self::find_variable_references(operand));
            }
            Expr::Binary(_, left, right) => {
                vars.extend(Self::find_variable_references(left));
                vars.extend(Self::find_variable_references(right));
            }
            Expr::Assignment(lhs, _, rhs) => {
                vars.extend(Self::find_variable_references(lhs));
                vars.extend(Self::find_variable_references(rhs));
            }
            Expr::FunCall(_, args) => {
                for arg in args {
                    vars.extend(Self::find_variable_references(arg));
                }
            }
            Expr::Ternary(cond, true_expr, false_expr) => {
                vars.extend(Self::find_variable_references(cond));
                vars.extend(Self::find_variable_references(true_expr));
                vars.extend(Self::find_variable_references(false_expr));
            }
            Expr::Bracket(base, _index_spec) => {
                // Array indexing not supported, but we can still find variables in base
                vars.extend(Self::find_variable_references(base));
            }
            Expr::Dot(base, _) => {
                vars.extend(Self::find_variable_references(base));
            }
            Expr::PostInc(operand) | Expr::PostDec(operand) => {
                vars.extend(Self::find_variable_references(operand));
            }
            Expr::Comma(left, right) => {
                vars.extend(Self::find_variable_references(left));
                vars.extend(Self::find_variable_references(right));
            }
            _ => {
                // Literals and other expressions don't reference variables
            }
        }
        vars
    }

    /// Find all variable names referenced in a statement.
    fn find_variable_references_in_statement(
        stmt: &Statement,
    ) -> alloc::collections::BTreeSet<String> {
        let mut vars = alloc::collections::BTreeSet::new();
        match stmt {
            Statement::Simple(simple) => {
                match simple.as_ref() {
                    SimpleStatement::Expression(expr_opt) => {
                        if let Some(expr) = expr_opt {
                            vars.extend(Self::find_variable_references(expr));
                        }
                    }
                    SimpleStatement::Selection(sel) => {
                        vars.extend(Self::find_variable_references(&sel.cond));
                        match &sel.rest {
                            SelectionRestStatement::Statement(true_stmt) => {
                                vars.extend(Self::find_variable_references_in_statement(true_stmt));
                            }
                            SelectionRestStatement::Else(true_stmt, false_stmt) => {
                                vars.extend(Self::find_variable_references_in_statement(true_stmt));
                                vars.extend(Self::find_variable_references_in_statement(
                                    false_stmt,
                                ));
                            }
                        }
                    }
                    SimpleStatement::Iteration(iter) => {
                        match iter {
                            IterationStatement::While(cond, body) => {
                                match cond {
                                    glsl::syntax::Condition::Expr(expr) => {
                                        vars.extend(Self::find_variable_references(expr));
                                    }
                                    _ => {}
                                }
                                vars.extend(Self::find_variable_references_in_statement(body));
                            }
                            IterationStatement::DoWhile(body, cond_expr) => {
                                vars.extend(Self::find_variable_references(cond_expr));
                                vars.extend(Self::find_variable_references_in_statement(body));
                            }
                            IterationStatement::For(init, rest, body) => {
                                match init {
                                    ForInitStatement::Expression(expr_opt) => {
                                        if let Some(expr) = expr_opt {
                                            vars.extend(Self::find_variable_references(expr));
                                        }
                                    }
                                    ForInitStatement::Declaration(decl) => {
                                        // Declaration doesn't reference variables (it declares them)
                                    }
                                }
                                if let Some(cond) = &rest.condition {
                                    match cond {
                                        glsl::syntax::Condition::Expr(expr) => {
                                            vars.extend(Self::find_variable_references(expr));
                                        }
                                        _ => {}
                                    }
                                }
                                if let Some(post_expr) = &rest.post_expr {
                                    vars.extend(Self::find_variable_references(post_expr));
                                }
                                vars.extend(Self::find_variable_references_in_statement(body));
                            }
                        }
                    }
                    _ => {}
                }
            }
            Statement::Compound(compound) => {
                for stmt in &compound.statement_list {
                    vars.extend(Self::find_variable_references_in_statement(stmt));
                }
            }
        }
        vars
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::String;

    use super::*;
    use crate::{parser::parse_glsl, typecheck::TypeChecker};

    #[test]
    fn test_codegen_simple_add() {
        let glsl = r#"
            int add(int x, int y) {
                return x + y;
            }
        "#;

        // Parse
        let functions = parse_glsl(glsl).unwrap();
        assert_eq!(functions.len(), 1);

        // Type check - register function signature
        let mut checker = TypeChecker::new();
        checker.register_functions(&functions).unwrap();
        checker
            .type_check_function_body(&functions[0].definition)
            .unwrap();

        // Generate code
        let sig = Signature::new(vec![Type::I32, Type::I32], vec![Type::I32]);
        let mut codegen = CodeGen::new(String::from("add"), sig);
        codegen
            .generate_function(&functions[0].definition, checker.symbols())
            .unwrap();

        let func = codegen.finish();
        assert_eq!(func.name(), "add");
        assert!(func.block_count() > 0);
    }

    #[test]
    fn test_codegen_int_literal() {
        let glsl = r#"
            int main() {
                return 42;
            }
        "#;

        let functions = parse_glsl(glsl).unwrap();
        let mut checker = TypeChecker::new();
        checker.register_functions(&functions).unwrap();
        checker
            .type_check_function_body(&functions[0].definition)
            .unwrap();

        let sig = Signature::new(vec![], vec![Type::I32]);
        let mut codegen = CodeGen::new(String::from("main"), sig);
        codegen
            .generate_function(&functions[0].definition, checker.symbols())
            .unwrap();

        let func = codegen.finish();
        assert_eq!(func.name(), "main");
    }
}
