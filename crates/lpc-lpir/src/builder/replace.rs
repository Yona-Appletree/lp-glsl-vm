//! Replace builder for replacing existing instructions.

use crate::{
    builder::traits::InstBuilderBase,
    dfg::{InstData, DFG},
    entity::Inst as InstEntity,
    value::Value,
};

/// Builder for replacing existing instructions.
///
/// This builder allows replacing the data of an existing instruction while
/// preserving its `InstEntity` ID. This is useful for optimizations and
/// transformations that need to modify instructions in place.
pub struct ReplaceBuilder<'f> {
    dfg: &'f mut DFG,
    inst: InstEntity,
    reuse_results: Option<alloc::vec::Vec<Value>>,
}

impl<'f> ReplaceBuilder<'f> {
    /// Create a new replace builder for the given instruction.
    ///
    /// The instruction must already exist in the DFG.
    pub fn new(dfg: &'f mut DFG, inst: InstEntity) -> Self {
        Self {
            dfg,
            inst,
            reuse_results: None,
        }
    }

    /// Specify result values to reuse for the replacement instruction.
    ///
    /// This allows reusing existing values instead of creating new ones.
    /// The values must match the number of results the instruction produces.
    pub fn with_results(mut self, results: alloc::vec::Vec<Value>) -> Self {
        self.reuse_results = Some(results);
        self
    }

    /// Specify a single result value to reuse for the replacement instruction.
    ///
    /// This is a convenience method for instructions with a single result.
    pub fn with_result(mut self, result: Value) -> Self {
        self.reuse_results = Some(alloc::vec![result]);
        self
    }
}

impl<'f> InstBuilderBase<'f> for ReplaceBuilder<'f> {
    fn data_flow_graph(&self) -> &DFG {
        self.dfg
    }

    fn data_flow_graph_mut(&mut self) -> &mut DFG {
        self.dfg
    }

    fn build(mut self, mut data: InstData) -> (InstEntity, &'f mut DFG) {
        // If we have reuse results, replace the results in the instruction data
        if let Some(reuse_results) = self.reuse_results.take() {
            if reuse_results.len() == data.results.len() {
                data.results = reuse_results;
            }
            // If lengths don't match, use the original results
        }

        // Replace the instruction data while preserving the InstEntity ID
        if let Some(inst_data) = self.dfg.inst_data_mut(self.inst) {
            *inst_data = data;
        }

        (self.inst, self.dfg)
    }
}

// ReplaceBuilder automatically gets InstBuilder methods via the blanket implementation
// in traits.rs: impl<'f, T: InstBuilderBase<'f>> InstBuilder<'f> for T {}

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
        Function,
    };

    #[test]
    fn test_replace_builder_replace_instruction() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, alloc::string::String::from("test"));
        let block = func.create_block();
        func.append_block(block);

        // Create initial instruction
        let v1 = Value::new(1);
        let v2 = Value::new(2);
        let v3 = Value::new(3);
        func.dfg.set_value_type(v1, Type::I32);
        func.dfg.set_value_type(v2, Type::I32);

        let inst_data1 = InstData::arithmetic(Opcode::Iadd, v3, v1, v2);
        let inst = func.create_inst(inst_data1);
        func.append_inst(inst, block);

        // Replace with subtraction
        let builder = ReplaceBuilder::new(&mut func.dfg, inst);
        builder.isub(v1, v2);

        // Verify instruction was replaced
        let inst_data = func.dfg.inst_data(inst).unwrap();
        assert_eq!(inst_data.opcode, Opcode::Isub);
        assert_eq!(inst_data.args, vec![v1, v2]);
        // Results should be updated to new value
        assert_eq!(inst_data.results.len(), 1);
    }

    #[test]
    fn test_replace_builder_with_result() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, alloc::string::String::from("test"));
        let block = func.create_block();
        func.append_block(block);

        // Create initial instruction
        let v1 = Value::new(1);
        let v2 = Value::new(2);
        let v3 = Value::new(3);
        func.dfg.set_value_type(v1, Type::I32);
        func.dfg.set_value_type(v2, Type::I32);

        let inst_data1 = InstData::arithmetic(Opcode::Iadd, v3, v1, v2);
        let inst = func.create_inst(inst_data1);
        func.append_inst(inst, block);

        // Replace with reuse result
        let reuse_result = Value::new(10);
        let builder = ReplaceBuilder::new(&mut func.dfg, inst).with_result(reuse_result);
        builder.isub(v1, v2);

        // Verify instruction uses reused result
        let inst_data = func.dfg.inst_data(inst).unwrap();
        assert_eq!(inst_data.opcode, Opcode::Isub);
        assert_eq!(inst_data.results, vec![reuse_result]);
    }

    #[test]
    fn test_replace_builder_preserves_inst_entity() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, alloc::string::String::from("test"));
        let block = func.create_block();
        func.append_block(block);

        // Create initial instruction
        let v1 = Value::new(1);
        let v2 = Value::new(2);
        let v3 = Value::new(3);
        func.dfg.set_value_type(v1, Type::I32);
        func.dfg.set_value_type(v2, Type::I32);

        let inst_data1 = InstData::arithmetic(Opcode::Iadd, v3, v1, v2);
        let inst = func.create_inst(inst_data1);
        func.append_inst(inst, block);

        let inst_index = inst.index();

        // Replace instruction
        let builder = ReplaceBuilder::new(&mut func.dfg, inst);
        builder.imul(v1, v2);

        // Verify same InstEntity is used
        let insts: Vec<_> = func.block_insts(block).collect();
        assert_eq!(insts.len(), 1);
        assert_eq!(insts[0].index(), inst_index);
    }
}
