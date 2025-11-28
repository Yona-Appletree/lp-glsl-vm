//! Insert builder for inserting instructions into a function.

use crate::{
    builder::traits::{InstBuilderBase, InstInserterBase},
    dfg::{InstData, DFG},
    entity::{Block, Inst as InstEntity},
    value::Value,
    Function,
};

/// Builder for inserting instructions into a function.
///
/// This builder wraps an `InstInserterBase` and provides the `InstBuilderBase`
/// interface, allowing instructions to be built and inserted into the layout.
///
/// It also provides `with_result()` and `with_results()` methods for value reuse.
pub struct InsertBuilder<'f, I: InstInserterBase<'f>> {
    inserter: I,
    reuse_results: Option<alloc::vec::Vec<Value>>,
    _phantom: core::marker::PhantomData<&'f ()>,
}

impl<'f, I: InstInserterBase<'f>> InsertBuilder<'f, I> {
    /// Create a new insert builder wrapping the given inserter.
    pub fn new(inserter: I) -> Self {
        Self {
            inserter,
            reuse_results: None,
            _phantom: core::marker::PhantomData,
        }
    }

    /// Specify result values to reuse for the next instruction.
    ///
    /// This allows reusing existing values instead of creating new ones.
    /// The values must match the number of results the instruction produces.
    pub fn with_results(mut self, results: alloc::vec::Vec<Value>) -> Self {
        self.reuse_results = Some(results);
        self
    }

    /// Specify a single result value to reuse for the next instruction.
    ///
    /// This is a convenience method for instructions with a single result.
    pub fn with_result(mut self, result: Value) -> Self {
        self.reuse_results = Some(alloc::vec![result]);
        self
    }
}

impl<'f, I: InstInserterBase<'f>> InstBuilderBase<'f> for InsertBuilder<'f, I> {
    fn data_flow_graph(&self) -> &DFG {
        self.inserter.data_flow_graph()
    }

    fn data_flow_graph_mut(&mut self) -> &mut DFG {
        self.inserter.data_flow_graph_mut()
    }

    fn build(mut self, mut data: InstData) -> (InstEntity, &'f mut DFG) {
        // If we have reuse results, replace the results in the instruction data
        if let Some(reuse_results) = self.reuse_results.take() {
            if reuse_results.len() == data.results.len() {
                data.results = reuse_results;
            }
            // If lengths don't match, use the original results
        }

        // Create the instruction in the DFG
        let inst = self.data_flow_graph_mut().create_inst(data);

        // Insert it into the layout using the inserter
        let dfg = self.inserter.insert_built_inst(inst);

        (inst, dfg)
    }
}

// InsertBuilder automatically gets InstBuilder methods via the blanket implementation
// in traits.rs: impl<'f, T: InstBuilderBase<'f>> InstBuilder<'f> for T {}

/// Concrete inserter that appends instructions to a block.
///
/// This is a simple implementation of `InstInserterBase` that appends
/// instructions to the end of a block.
pub struct BlockAppendInserter<'f> {
    function: &'f mut Function,
    block: Block,
}

impl<'f> BlockAppendInserter<'f> {
    /// Create a new inserter that appends to the given block.
    pub fn new(function: &'f mut Function, block: Block) -> Self {
        Self { function, block }
    }
}

impl<'f> InstInserterBase<'f> for BlockAppendInserter<'f> {
    fn data_flow_graph(&self) -> &DFG {
        &self.function.dfg
    }

    fn data_flow_graph_mut(&mut self) -> &mut DFG {
        &mut self.function.dfg
    }

    fn insert_built_inst(self, inst: InstEntity) -> &'f mut DFG {
        // Ensure the instruction is in the layout
        self.function.layout.ensure_inst(inst);
        // Append it to the block
        self.function.layout.append_inst(inst, self.block);
        &mut self.function.dfg
    }
}

#[cfg(test)]
mod tests {
    use alloc::{vec, vec::Vec};

    use super::*;
    use crate::{
        builder::traits::InstBuilder,
        dfg::{InstData, Opcode},
        signature::Signature,
        types::Type,
        value::Value,
    };

    #[test]
    fn test_insert_builder_arithmetic() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, alloc::string::String::from("test"));
        let block = func.create_block();
        func.append_block(block);

        let v1 = Value::new(1);
        let v2 = Value::new(2);
        func.dfg.set_value_type(v1, Type::I32);
        func.dfg.set_value_type(v2, Type::I32);

        let inserter = BlockAppendInserter::new(&mut func, block);
        let builder = InsertBuilder::new(inserter);

        let result = builder.iadd(v1, v2);
        assert_eq!(result.index(), 3); // Next value index

        // Verify instruction was created and inserted
        let insts: Vec<_> = func.block_insts(block).collect();
        assert_eq!(insts.len(), 1);
        let inst_data = func.dfg.inst_data(insts[0]).unwrap();
        assert_eq!(inst_data.opcode, Opcode::Iadd);
        assert_eq!(inst_data.args, vec![v1, v2]);
        assert_eq!(inst_data.results, vec![result]);
    }

    #[test]
    fn test_insert_builder_with_result() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, alloc::string::String::from("test"));
        let block = func.create_block();
        func.append_block(block);

        let v1 = Value::new(1);
        let v2 = Value::new(2);
        let reuse_result = Value::new(10);
        func.dfg.set_value_type(v1, Type::I32);
        func.dfg.set_value_type(v2, Type::I32);

        let inserter = BlockAppendInserter::new(&mut func, block);
        let builder = InsertBuilder::new(inserter).with_result(reuse_result);

        let result = builder.iadd(v1, v2);
        // Note: iadd returns the value it created, but the instruction will use the reused result

        // Verify instruction uses the reused result (not the one returned)
        let insts: Vec<_> = func.block_insts(block).collect();
        let inst_data = func.dfg.inst_data(insts[0]).unwrap();
        assert_eq!(inst_data.results, vec![reuse_result]);
        // The returned value is what was created, but the instruction uses the reused one
        assert_ne!(result, reuse_result); // They're different values
    }

    #[test]
    fn test_insert_builder_constant() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, alloc::string::String::from("test"));
        let block = func.create_block();
        func.append_block(block);

        let inserter = BlockAppendInserter::new(&mut func, block);
        let builder = InsertBuilder::new(inserter);

        let result = builder.iconst(42);
        assert_eq!(result.index(), 1);

        let insts: Vec<_> = func.block_insts(block).collect();
        let inst_data = func.dfg.inst_data(insts[0]).unwrap();
        assert_eq!(inst_data.opcode, Opcode::Iconst);
        assert_eq!(inst_data.results, vec![result]);
    }

    #[test]
    fn test_insert_builder_load() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, alloc::string::String::from("test"));
        let block = func.create_block();
        func.append_block(block);

        let addr = Value::new(1);
        func.dfg.set_value_type(addr, Type::I32);

        let inserter = BlockAppendInserter::new(&mut func, block);
        let builder = InsertBuilder::new(inserter);

        let result = builder.load(addr, Type::I32);
        assert_eq!(result.index(), 2);

        let insts: Vec<_> = func.block_insts(block).collect();
        let inst_data = func.dfg.inst_data(insts[0]).unwrap();
        assert_eq!(inst_data.opcode, Opcode::Load);
        assert_eq!(inst_data.args, vec![addr]);
        assert_eq!(inst_data.results, vec![result]);
        assert_eq!(inst_data.ty, Some(Type::I32));
    }
}
