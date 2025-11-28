//! Functions.

use alloc::{string::String, vec::Vec};
use core::fmt;

use crate::{
    block::BlockData,
    dfg::DFG,
    entity::{Block, Inst},
    entity_map::PrimaryMap,
    layout::Layout,
    signature::Signature,
};

/// A function in the IR
///
/// A function consists of:
/// - A signature (parameters and return types)
/// - Block data (what blocks are - parameters)
/// - Layout (where blocks/instructions are)
/// - DFG (what instructions are - opcode + operands)
/// - A name (required, for debugging and module lookup)
#[derive(Debug, Clone)]
pub struct Function {
    /// Function signature
    pub signature: Signature,
    /// Function name
    pub name: String,
    /// Block data (what blocks are)
    pub blocks: PrimaryMap<Block, BlockData>,
    /// Layout (where blocks/instructions are)
    pub layout: Layout,
    /// Data Flow Graph (what instructions are)
    pub dfg: DFG,
}

impl Function {
    /// Create a new function with the given signature and name
    pub fn new(signature: Signature, name: String) -> Self {
        Self {
            signature,
            name,
            blocks: PrimaryMap::new(),
            layout: Layout::new(),
            dfg: DFG::new(),
        }
    }

    /// Set the function name
    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }

    /// Get the function name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Create a new block and return its entity
    ///
    /// The block is created with empty parameters. Use `append_block` to add it to the layout.
    pub fn create_block(&mut self) -> Block {
        let block_data = BlockData::new();
        let block = self.blocks.push(block_data);
        self.layout.ensure_block(block);
        block
    }

    /// Create a new block with parameters and return its entity
    ///
    /// The block is created with the given parameters. Use `append_block` to add it to the layout.
    pub fn create_block_with_params(&mut self, params: Vec<crate::value::Value>) -> Block {
        let block_data = BlockData::with_params(params);
        let block = self.blocks.push(block_data);
        self.layout.ensure_block(block);
        block
    }

    /// Create an instruction and return its entity
    ///
    /// The instruction is created in the DFG but not yet inserted into the layout.
    /// Use `append_inst` or `insert_inst` to add it to a block.
    pub fn create_inst(&mut self, data: crate::dfg::InstData) -> Inst {
        let inst = self.dfg.create_inst(data);
        self.layout.ensure_inst(inst);
        inst
    }

    /// Append a block to the end of the layout
    pub fn append_block(&mut self, block: Block) {
        self.layout.append_block(block);
    }

    /// Append an instruction to the end of a block
    pub fn append_inst(&mut self, inst: Inst, block: Block) {
        self.layout.append_inst(inst, block);
    }

    /// Get the entry block (first block in layout order)
    pub fn entry_block(&self) -> Option<Block> {
        self.layout.entry_block()
    }

    /// Get block data
    pub fn block_data(&self, block: Block) -> Option<&BlockData> {
        self.blocks.get(block)
    }

    /// Get mutable block data
    pub fn block_data_mut(&mut self, block: Block) -> Option<&mut BlockData> {
        self.blocks.get_mut(block)
    }

    /// Get the number of blocks in this function
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    /// Get an iterator over blocks in layout order
    pub fn blocks(&self) -> impl Iterator<Item = Block> + '_ {
        self.layout.blocks()
    }

    /// Get an iterator over instructions in a block
    pub fn block_insts(&self, block: Block) -> impl Iterator<Item = Inst> + '_ {
        self.layout.block_insts(block)
    }
}

impl fmt::Display for Function {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Function name
        write!(f, "function %{}", self.name)?;

        // Signature
        write!(f, "(")?;
        for (i, param_ty) in self.signature.params.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", param_ty)?;
        }
        write!(f, ")")?;

        if !self.signature.returns.is_empty() {
            write!(f, " -> ")?;
            for (i, ret_ty) in self.signature.returns.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{}", ret_ty)?;
            }
        }

        writeln!(f, " {{")?;

        // Print each block with inline parameters (Cranelift format)
        for (block_idx, block) in self.blocks().enumerate() {
            // Format block header with parameters
            write!(f, "block{}", block_idx)?;
            if let Some(block_data) = self.block_data(block) {
                if !block_data.params.is_empty() {
                    write!(f, "(")?;
                    for (i, param) in block_data.params.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        // Default to i32 for parameter types (as per plan)
                        // TODO: Could enhance later to track actual types
                        write!(f, "v{}: i32", param.index())?;
                    }
                    write!(f, ")")?;
                }
            }
            writeln!(f, ":")?;

            // Print instructions in this block
            for inst in self.block_insts(block) {
                if let Some(inst_data) = self.dfg.inst_data(inst) {
                    // TODO: Implement proper instruction formatting
                    writeln!(f, "    {:?}", inst_data.opcode)?;
                }
            }
        }

        writeln!(f, "}}")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use crate::{
        dfg::{InstData, Opcode},
        types::Type,
        value::Value,
    };

    #[test]
    fn test_function_new() {
        let sig = Signature::new(vec![Type::I32, Type::I32], vec![Type::I32]);
        let func = Function::new(sig.clone(), String::from("test"));
        assert_eq!(func.block_count(), 0);
        assert_eq!(func.name(), "test");
    }

    #[test]
    fn test_function_create_block() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let block = func.create_block();
        assert_eq!(func.block_count(), 1);
        assert!(func.block_data(block).is_some());
    }

    #[test]
    fn test_function_create_block_with_params() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let params = vec![Value::new(0), Value::new(1)];
        let block = func.create_block_with_params(params.clone());
        assert_eq!(func.block_count(), 1);
        let block_data = func.block_data(block).unwrap();
        assert_eq!(block_data.params.len(), 2);
    }

    #[test]
    fn test_function_create_inst() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let v1 = Value::new(1);
        let v2 = Value::new(2);
        let v3 = Value::new(3);
        let inst_data = InstData::arithmetic(Opcode::Iadd, v3, v1, v2);
        let inst = func.create_inst(inst_data);
        assert!(func.dfg.inst_data(inst).is_some());
    }

    #[test]
    fn test_function_entry_block() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let block = func.create_block();
        func.append_block(block);
        assert!(func.entry_block().is_some());
        assert_eq!(func.entry_block(), Some(block));
    }

    #[test]
    fn test_function_block_insts() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let block = func.create_block();
        func.append_block(block);

        let v1 = Value::new(1);
        let v2 = Value::new(2);
        let v3 = Value::new(3);
        let inst_data = InstData::arithmetic(Opcode::Iadd, v3, v1, v2);
        let inst = func.create_inst(inst_data);
        func.append_inst(inst, block);

        let insts: Vec<_> = func.block_insts(block).collect();
        assert_eq!(insts.len(), 1);
        assert_eq!(insts[0], inst);
    }
}
