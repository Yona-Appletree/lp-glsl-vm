//! Code generation for function-level constructs.

use alloc::{
    boxed::Box,
    collections::{BTreeMap, BTreeSet},
    format,
    string::String,
    vec,
    vec::Vec,
};

use glsl::syntax::FunctionDefinition;
use lpc_lpir::{BlockEntity, Function, FunctionBuilder, Opcode, Signature, Type, Value};

use crate::{
    codegen::{CodeGenBuilder, LoopStack, SSABuilder, ScopeStack},
    decl::codegen::generate_declaration,
    error::{GlslError, GlslResult},
    expr::codegen::generate_expr,
    stmt::codegen::generate_statement,
    symbols::SymbolTable,
    types::GlslType,
    util::extract_type_from_specifier,
};

/// Code generator context trait for accessing CodeGen internals.
pub trait CodeGenContext {
    fn current_block(&self) -> GlslResult<BlockEntity>;
    fn builder_mut(&mut self) -> &mut FunctionBuilder;
    fn variables(&self) -> &BTreeMap<String, Value>;
    fn variables_mut(&mut self) -> &mut BTreeMap<String, Value>;
    fn symbols(&self) -> &SymbolTable;
    fn set_current_block(&mut self, block: BlockEntity);
    fn scope_stack_mut(&mut self) -> &mut Vec<BTreeSet<String>>;
    fn out_inout_params(&self) -> &BTreeMap<String, (Value, GlslType)>;
    fn block_ends_with_return_or_halt(&mut self, block: BlockEntity) -> bool;
    /// Clone the current variables map (for saving state in control flow).
    fn clone_variables(&self) -> BTreeMap<String, Value>;
    /// Restore variables map from a clone.
    fn restore_variables(&mut self, vars: BTreeMap<String, Value>);

    // New methods for accessing new abstractions
    /// Get a CodeGenBuilder for the current block.
    fn codegen_builder(&mut self) -> GlslResult<CodeGenBuilder>;
    /// Get the SSA builder.
    fn ssa_builder_mut(&mut self) -> &mut SSABuilder;
    /// Get the loop stack.
    fn loop_stack_mut(&mut self) -> &mut LoopStack;
    /// Get the new scope stack.
    fn scope_stack_new_mut(&mut self) -> &mut ScopeStack;
}

/// Code generator context.
///
/// This holds the function builder and tracks SSA values for variables.
pub struct CodeGen {
    /// Function builder for constructing LPIR
    builder: FunctionBuilder,
    /// Current block being built
    current_block: Option<BlockEntity>,
    /// Variable name to SSA value mapping (for current scope)
    variables: BTreeMap<String, Value>,
    /// Symbol table for function lookups
    symbols: SymbolTable,
    /// Scope stack: each entry is a set of variable names declared in that scope
    /// (Legacy - will be replaced by scope_stack_new)
    scope_stack: Vec<BTreeSet<String>>,
    /// New scope stack with RAII support
    scope_stack_new: ScopeStack,
    /// SSA builder for proper SSA construction
    ssa_builder: SSABuilder,
    /// Loop stack for tracking nested loops
    loop_stack: LoopStack,
    /// Out/inout parameter tracking: variable name -> (address_param, type)
    out_inout_params: BTreeMap<String, (Value, GlslType)>,
}

impl CodeGenContext for CodeGen {
    fn current_block(&self) -> GlslResult<BlockEntity> {
        self.current_block
            .ok_or_else(|| GlslError::codegen("Internal error: no current block set"))
    }

    fn builder_mut(&mut self) -> &mut FunctionBuilder {
        &mut self.builder
    }

    fn variables(&self) -> &BTreeMap<String, Value> {
        &self.variables
    }

    fn variables_mut(&mut self) -> &mut BTreeMap<String, Value> {
        &mut self.variables
    }

    fn symbols(&self) -> &SymbolTable {
        &self.symbols
    }

    fn set_current_block(&mut self, block: BlockEntity) {
        self.current_block = Some(block);
    }

    fn scope_stack_mut(&mut self) -> &mut Vec<BTreeSet<String>> {
        &mut self.scope_stack
    }

    fn out_inout_params(&self) -> &BTreeMap<String, (Value, GlslType)> {
        &self.out_inout_params
    }

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

    fn clone_variables(&self) -> BTreeMap<String, Value> {
        self.variables.clone()
    }

    fn restore_variables(&mut self, vars: BTreeMap<String, Value>) {
        self.variables = vars;
    }

    fn codegen_builder(&mut self) -> GlslResult<CodeGenBuilder> {
        let block = self.current_block()?;
        Ok(CodeGenBuilder::new(block, &mut self.builder))
    }

    fn ssa_builder_mut(&mut self) -> &mut SSABuilder {
        &mut self.ssa_builder
    }

    fn loop_stack_mut(&mut self) -> &mut LoopStack {
        &mut self.loop_stack
    }

    fn scope_stack_new_mut(&mut self) -> &mut ScopeStack {
        &mut self.scope_stack_new
    }
}

impl CodeGen {
    /// Record a variable definition in the current block.
    pub fn record_variable_def(&mut self, var: &str, value: Value) -> GlslResult<()> {
        let block = self.current_block()?;
        self.ssa_builder.record_def(var, block, value);
        // Also maintain legacy tracking for backward compatibility
        self.variables.insert(String::from(var), value);
        Ok(())
    }

    /// Get a variable value using SSABuilder with fallback.
    pub fn get_variable_value(&mut self, var: &str) -> GlslResult<Value> {
        let block = self.current_block()?;
        if let Some(value) = self.ssa_builder.get_value(var, block) {
            Ok(value)
        } else if let Some(value) = self.variables.get(var) {
            Ok(*value)
        } else {
            Err(GlslError::codegen(format!("Undefined variable '{}'", var)))
        }
    }
}

impl CodeGen {
    /// Create a new code generator for a function.
    pub fn new(name: String, signature: Signature) -> Self {
        let builder = FunctionBuilder::new(signature, name);
        Self {
            builder,
            current_block: None,
            variables: BTreeMap::new(),
            symbols: SymbolTable::new(),
            scope_stack: Vec::new(),
            scope_stack_new: ScopeStack::new(),
            ssa_builder: SSABuilder::new(),
            loop_stack: LoopStack::new(),
            out_inout_params: BTreeMap::new(),
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

        // Extract function signature to get parameter qualifiers
        let func_sig = crate::function::typecheck::extract_function_signature(func_def)?;

        // Create entry block with parameters
        let mut entry_params = Vec::new();
        let mut param_types = Vec::new();

        for (idx, param_decl) in func_def.prototype.parameters.iter().enumerate() {
            let param_idx = entry_params.len() as u32;
            let param_value = Value::new(param_idx);
            entry_params.push(param_value);

            // Extract parameter type and qualifier
            if let glsl::syntax::FunctionParameterDeclaration::Named(_, declarator) = param_decl {
                let glsl_type = extract_type_from_specifier(&declarator.ty)
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
        let body_stmt = glsl::syntax::Statement::Compound(Box::new(func_def.statement.clone()));
        generate_statement(self, &body_stmt)?;

        // Compute dominance tree for SSABuilder (for dominance-aware value lookup)
        // This allows SSABuilder to find values that dominate merge points
        // We can do this during codegen now that FunctionBuilder exposes function()
        let function = self.builder.function();
        self.ssa_builder.compute_dominance(function);

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

    /// Create a default value for a type.
    fn create_default_value(&mut self, ty: GlslType) -> GlslResult<Value> {
        let block = self.current_block()?;
        let value = self.builder.new_value();
        let mut block_builder = self.builder.block_builder(block);
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
}
