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
        }
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

        // Create entry block with parameters
        let mut entry_params = Vec::new();
        let mut param_types = Vec::new();
        
        for param_decl in &func_def.prototype.parameters {
            let param_idx = entry_params.len() as u32;
            let param_value = Value::new(param_idx);
            entry_params.push(param_value);
            
            // Extract parameter type
            if let glsl::syntax::FunctionParameterDeclaration::Named(_, declarator) = param_decl {
                let glsl_type = Self::extract_type_from_specifier(&declarator.ty)
                    .ok_or_else(|| GlslError::codegen("Unsupported parameter type"))?;
                let lpir_type = glsl_type.to_lpir();
                param_types.push(lpir_type);
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
                self.builder.function_mut().dfg.set_value_type(*param_value, *param_type);
            }
        }
        
        self.current_block = Some(entry_block);

        // Advance SSA counter past parameters
        for _ in 0..entry_params.len() {
            let _ = self.builder.new_value();
        }

        // Add parameters to variable map
        for (idx, param_decl) in func_def.prototype.parameters.iter().enumerate() {
            if let glsl::syntax::FunctionParameterDeclaration::Named(_, declarator) = param_decl {
                let name = declarator.ident.ident.0.clone();
                self.variables.insert(name, entry_params[idx]);
            }
        }

        // Generate code for function body
        let body_stmt = Statement::Compound(Box::new(func_def.statement.clone()));
        self.generate_statement(&body_stmt)?;

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
        // Push new scope for variables
        let old_variables = self.variables.clone();

        // Generate each statement
        for stmt in &compound.statement_list {
            self.generate_statement(stmt)?;
        }

        // Restore previous scope (pop variables that were declared in this scope)
        // For simplicity, we'll keep all variables (shadowing is handled by lookup)
        // In a more sophisticated implementation, we'd track scope depth
        self.variables = old_variables;

        Ok(())
    }

    /// Generate LPIR for a simple statement.
    fn generate_simple_statement(&mut self, simple: &SimpleStatement) -> GlslResult<()> {
        let block = self.current_block.expect("No current block");
        let mut block_builder = self.builder.block_builder(block);

        match simple {
            SimpleStatement::Declaration(decl) => {
                self.generate_declaration(decl)?;
                Ok(())
            }
            SimpleStatement::Expression(expr_stmt) => {
                if let Some(expr) = expr_stmt {
                    self.generate_expr(expr)?;
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

                    self.variables.insert(name.0.clone(), value);
                }

                // Declare tail variables
                for decl_no_type in &list.tail {
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

                    self.variables
                        .insert(decl_no_type.ident.ident.0.clone(), value);
                }

                Ok(())
            }
            _ => Err(GlslError::codegen("Unsupported declaration type")),
        }
    }

    /// Create a default value for a type.
    fn create_default_value(&mut self, ty: GlslType) -> GlslResult<Value> {
        let block = self.current_block.expect("No current block");
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
                let block = self.current_block.expect("No current block");
                let value = self.builder.new_value();
                let mut block_builder = self.builder.block_builder(block);
                block_builder.iconst(value, *i as i64);
                Ok(value)
            }
            Expr::BoolConst(b) => {
                let block = self.current_block.expect("No current block");
                let value = self.builder.new_value();
                let mut block_builder = self.builder.block_builder(block);
                block_builder.iconst(value, if *b { 1 } else { 0 });
                // Bool maps to u32 in LPIR, so set the type explicitly
                drop(block_builder);
                self.builder.function_mut().dfg.set_value_type(value, Type::U32);
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
                // For now, we only support variable assignments
                if let Expr::Variable(ident) = lhs.as_ref() {
                    let name = ident.0.clone();
                    self.variables.insert(name, rhs_value);
                    Ok(rhs_value)
                } else {
                    Err(GlslError::codegen("Complex assignment not supported"))
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

                // Generate argument values
                let mut arg_values = Vec::new();
                for arg_expr in args {
                    arg_values.push(self.generate_expr(arg_expr)?);
                }

                // Get function signature to determine return type
                let sig = self
                    .symbols
                    .lookup_function(name)
                    .ok_or_else(|| GlslError::codegen(format!("Undefined function '{}'", name)))?;

                // Generate return value(s)
                let mut return_values = Vec::new();
                if let Some(_return_ty) = &sig.return_type {
                    let return_value = self.builder.new_value();
                    return_values.push(return_value);
                }

                // Generate call instruction
                let block = self.current_block.expect("No current block");
                let mut block_builder = self.builder.block_builder(block);
                block_builder.call(String::from(name), arg_values, return_values.clone());

                // Return the first return value (or error if void)
                return_values
                    .first()
                    .copied()
                    .ok_or_else(|| GlslError::codegen(format!("Function '{}' returns void", name)))
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
        let block = self.current_block.expect("No current block");
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
                self.builder.function_mut().dfg.set_value_type(zero, Type::U32);
                let mut block_builder = self.builder.block_builder(block);
                block_builder.icmp_eq(result, operand, zero);
                // Bool maps to u32 in LPIR
                drop(block_builder);
                self.builder.function_mut().dfg.set_value_type(result, Type::U32);
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
        let block = self.current_block.expect("No current block");
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
                self.builder.function_mut().dfg.set_value_type(result, Type::U32);
                Ok(result)
            }
            glsl::syntax::BinaryOp::GT => {
                block_builder.icmp_gt(result, left, right);
                // Bool maps to u32 in LPIR
                drop(block_builder);
                self.builder.function_mut().dfg.set_value_type(result, Type::U32);
                Ok(result)
            }
            glsl::syntax::BinaryOp::LTE => {
                block_builder.icmp_le(result, left, right);
                // Bool maps to u32 in LPIR
                drop(block_builder);
                self.builder.function_mut().dfg.set_value_type(result, Type::U32);
                Ok(result)
            }
            glsl::syntax::BinaryOp::GTE => {
                block_builder.icmp_ge(result, left, right);
                // Bool maps to u32 in LPIR
                drop(block_builder);
                self.builder.function_mut().dfg.set_value_type(result, Type::U32);
                Ok(result)
            }
            glsl::syntax::BinaryOp::Equal => {
                block_builder.icmp_eq(result, left, right);
                // Bool maps to u32 in LPIR
                drop(block_builder);
                self.builder.function_mut().dfg.set_value_type(result, Type::U32);
                Ok(result)
            }
            glsl::syntax::BinaryOp::NonEqual => {
                block_builder.icmp_ne(result, left, right);
                // Bool maps to u32 in LPIR
                drop(block_builder);
                self.builder.function_mut().dfg.set_value_type(result, Type::U32);
                Ok(result)
            }
            glsl::syntax::BinaryOp::And => {
                // Logical AND: both must be non-zero
                // Since bool is u32 (0 or 1), bitwise AND works perfectly for logical AND
                block_builder.iand(result, left, right);
                // Bool maps to u32 in LPIR
                drop(block_builder);
                self.builder.function_mut().dfg.set_value_type(result, Type::U32);
                Ok(result)
            }
            glsl::syntax::BinaryOp::Or => {
                // Logical OR: at least one must be non-zero
                // Since bool is u32 (0 or 1), bitwise OR works perfectly for logical OR
                block_builder.ior(result, left, right);
                // Bool maps to u32 in LPIR
                drop(block_builder);
                self.builder.function_mut().dfg.set_value_type(result, Type::U32);
                Ok(result)
            }
            _ => Err(GlslError::codegen("Unsupported binary operator")),
        }
    }

    /// Generate LPIR for a selection statement (if/else).
    fn generate_selection_statement(&mut self, sel: &SelectionStatement) -> GlslResult<()> {
        // Generate condition first (before getting block builder)
        let cond_value = self.generate_expr(&sel.cond)?;

        // Create blocks for true and false branches
        let block = self.current_block.expect("No current block");
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
                let true_end_block = self.current_block.expect("No current block after true branch");
                let true_ends_with_return_or_halt = self.block_ends_with_return_or_halt(true_end_block);
                
                // Create merge block - we always need it for the false branch
                let merge_block = self.builder.create_block();
                
                if !true_ends_with_return_or_halt {
                    // Need to jump to merge block from wherever we ended up
                    if true_end_block != true_block {
                        // Statement ended in a different block (e.g., for loop exit) - jump from there
                        let mut end_block_builder = self.builder.block_builder(true_end_block);
                        end_block_builder.jump(merge_block, &Vec::new());
                    } else {
                        // Statement ended in true_block - jump from there
                        let mut true_block_builder = self.builder.block_builder(true_block);
                        true_block_builder.jump(merge_block, &Vec::new());
                    }
                }
                
                // False branch is empty - jump directly to merge block
                let mut false_block_builder = self.builder.block_builder(false_block);
                false_block_builder.jump(merge_block, &Vec::new());
                
                // Continue in merge block
                self.current_block = Some(merge_block);
            }
            SelectionRestStatement::Else(true_stmt, false_stmt) => {
                self.generate_statement(true_stmt)?;
                // Check what block we ended up in after generating the statement
                let true_end_block = self.current_block.expect("No current block after true branch");
                let true_ends_with_return_or_halt = self.block_ends_with_return_or_halt(true_end_block);

                // Generate false branch
                self.current_block = Some(false_block);
                self.generate_statement(false_stmt)?;
                // Check what block we ended up in after generating the statement
                let false_end_block = self.current_block.expect("No current block after false branch");
                let false_ends_with_return_or_halt = self.block_ends_with_return_or_halt(false_end_block);
                
                // Only create merge block if at least one branch doesn't return/halt
                if !true_ends_with_return_or_halt || !false_ends_with_return_or_halt {
                    let merge_block = self.builder.create_block();
                    
                    if !true_ends_with_return_or_halt {
                        // Need to jump to merge block from wherever we ended up
                        if true_end_block != true_block {
                            // Statement ended in a different block (e.g., merge block from nested if) - jump from there
                            let mut end_block_builder = self.builder.block_builder(true_end_block);
                            end_block_builder.jump(merge_block, &Vec::new());
                        } else {
                            // Statement ended in true_block - jump from there
                            let mut true_block_builder = self.builder.block_builder(true_block);
                            true_block_builder.jump(merge_block, &Vec::new());
                        }
                    }
                    
                    if !false_ends_with_return_or_halt {
                        // Need to jump to merge block from wherever we ended up
                        if false_end_block != false_block {
                            // Statement ended in a different block - jump from there
                            let mut end_block_builder = self.builder.block_builder(false_end_block);
                            end_block_builder.jump(merge_block, &Vec::new());
                        } else {
                            // Statement ended in false_block - jump from there
                            let mut false_block_builder = self.builder.block_builder(false_block);
                            false_block_builder.jump(merge_block, &Vec::new());
                        }
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
        let block = self.current_block.expect("No current block");

        match iter {
            IterationStatement::While(cond, body) => {
                // Create blocks: condition, body, exit
                let cond_block = self.current_block.expect("No current block");
                let body_block = self.builder.create_block();
                let exit_block = self.builder.create_block();

                // Generate condition first (before getting block builder)
                let cond_value = match cond {
                    glsl::syntax::Condition::Expr(expr) => self.generate_expr(expr)?,
                    glsl::syntax::Condition::Assignment(_, _, _) => {
                        return Err(GlslError::codegen(
                            "Assignment in while condition not supported",
                        ))
                    }
                };

                // Branch: if condition, go to body, else exit
                let mut block_builder = self.builder.block_builder(cond_block);
                block_builder.br(cond_value, body_block, &Vec::new(), exit_block, &Vec::new());

                // Generate body
                self.current_block = Some(body_block);
                self.generate_statement(body)?;

                // Check what block we ended up in after generating the body
                let body_end_block = self.current_block.expect("No current block after body");
                
                // Jump back to condition (we'll need to regenerate condition)
                // For simplicity, we'll jump to a new condition block
                let new_cond_block = self.builder.create_block();
                
                // Jump from wherever the body ended (could be body_block or a merge block from nested control flow)
                if body_end_block != body_block {
                    // Body ended in a different block (e.g., merge block from nested if/while) - jump from there
                    let mut end_block_builder = self.builder.block_builder(body_end_block);
                    end_block_builder.jump(new_cond_block, &Vec::new());
                } else {
                    // Body ended in body_block - jump from there
                    let mut body_block_builder = self.builder.block_builder(body_block);
                    body_block_builder.jump(new_cond_block, &Vec::new());
                }

                // Generate condition again in new block
                self.current_block = Some(new_cond_block);
                let new_cond_value = match cond {
                    glsl::syntax::Condition::Expr(expr) => self.generate_expr(expr)?,
                    _ => unreachable!(),
                };
                let mut new_cond_builder = self.builder.block_builder(new_cond_block);
                new_cond_builder.br(
                    new_cond_value,
                    body_block,
                    &Vec::new(),
                    exit_block,
                    &Vec::new(),
                );

                // Continue in exit block
                self.current_block = Some(exit_block);

                Ok(())
            }
            IterationStatement::DoWhile(body, cond_expr) => {
                // Create blocks: body, condition, exit
                let body_block = self.builder.create_block();
                let cond_block = self.builder.create_block();
                let exit_block = self.builder.create_block();

                // Jump to body
                let mut block_builder = self.builder.block_builder(block);
                block_builder.jump(body_block, &Vec::new());

                // Generate body
                self.current_block = Some(body_block);
                self.generate_statement(body)?;

                // Jump to condition
                let mut body_block_builder = self.builder.block_builder(body_block);
                body_block_builder.jump(cond_block, &Vec::new());

                // Generate condition
                self.current_block = Some(cond_block);
                let cond_value = self.generate_expr(cond_expr)?;

                // Branch: if condition, go to body, else exit
                let mut cond_builder = self.builder.block_builder(cond_block);
                cond_builder.br(cond_value, body_block, &Vec::new(), exit_block, &Vec::new());

                // Continue in exit block
                self.current_block = Some(exit_block);

                Ok(())
            }
            IterationStatement::For(init, rest, body) => {
                // Create blocks: init, condition, body, increment, exit
                let init_block = self.current_block.expect("No current block");
                let cond_block = self.builder.create_block();
                let body_block = self.builder.create_block();
                let inc_block = self.builder.create_block();
                
                // Check if condition exists - if not, exit_block is unreachable
                let has_condition = rest.condition.is_some();
                let exit_block = if has_condition {
                    Some(self.builder.create_block())
                } else {
                    None
                };

                // Generate initialization
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

                // Jump to condition
                let mut block_builder = self.builder.block_builder(block);
                block_builder.jump(cond_block, &Vec::new());

                // Generate condition
                self.current_block = Some(cond_block);
                let cond_value = if let Some(cond) = &rest.condition {
                    match cond {
                        glsl::syntax::Condition::Expr(expr) => self.generate_expr(expr)?,
                        glsl::syntax::Condition::Assignment(_, _, _) => {
                            return Err(GlslError::codegen(
                                "Assignment in for condition not supported",
                            ))
                        }
                    }
                } else {
                    // No condition means always true
                    let true_val = self.builder.new_value();
                    let mut cond_builder = self.builder.block_builder(cond_block);
                    cond_builder.iconst(true_val, 1);
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
                let body_end_block = self.current_block.expect("No current block after body");
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

                // Jump back to condition
                let mut inc_builder = self.builder.block_builder(inc_block);
                inc_builder.jump(cond_block, &Vec::new());

                // Exit block will be continued by the next statement
                if let Some(exit) = exit_block {
                    self.current_block = Some(exit);
                } else {
                    // No exit block - body always continues, so current_block stays at inc_block
                    // But actually, after the jump, we're done with the loop
                    // The function should end with a return, so this is fine
                }

                Ok(())
            }
        }
    }

    /// Generate LPIR for a jump statement (return/break/continue).
    fn generate_jump_statement(&mut self, jump: &JumpStatement) -> GlslResult<()> {
        let block = self.current_block.expect("No current block");

        match jump {
            JumpStatement::Return(expr_opt) => {
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
    fn extract_type_from_specifier(
        ty: &glsl::syntax::TypeSpecifier,
    ) -> Option<GlslType> {
        GlslType::from_glsl_type_specifier(&ty.ty)
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
