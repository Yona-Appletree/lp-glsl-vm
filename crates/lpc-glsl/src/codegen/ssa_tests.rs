//! Tests for lazy dominance-based SSA construction.

#[cfg(test)]
mod tests {
    use alloc::{string::ToString, vec};
    
    use crate::codegen::SSABuilder;
    use lpc_lpir::{BlockEntity, Function, FunctionBuilder, Signature};

    fn create_test_function() -> Function {
        let sig = Signature::new(vec![], vec![]);
        let mut builder = FunctionBuilder::new(sig, "test".to_string());
        let _block0 = builder.create_block();
        let _block1 = builder.create_block();
        let _block2 = builder.create_block();
        let _block3 = builder.create_block();
        let _block4 = builder.create_block();
        builder.finish()
    }

    /// Test case: Variable defined in Block(1) and Block(3), read in Block(4)
    /// Block(4) should get value from Block(3) (its predecessor)
    #[test]
    fn test_lazy_dominance_simple_predecessor() {
        let mut ssa = SSABuilder::new();
        
        // Create a function with proper CFG structure
        let sig = Signature::new(vec![], vec![]);
        let mut builder = FunctionBuilder::new(sig, "test".to_string());
        
        // Create blocks
        let block1 = builder.create_block();
        let block3 = builder.create_block();
        let block4 = builder.create_block();
        
        // Set up CFG: Block(1) -> Block(3) -> Block(4)
        let mut b1 = builder.block_builder(block1);
        b1.jump(block3, &vec![]);
        drop(b1);
        
        let mut b3 = builder.block_builder(block3);
        b3.jump(block4, &vec![]);
        drop(b3);
        
        let function = builder.finish();
        
        // Create values
        let mut value_builder = FunctionBuilder::new(Signature::new(vec![], vec![]), "val_test".to_string());
        let value1 = value_builder.new_value();
        let value3 = value_builder.new_value();
        
        // Record 'i' in Block(1) and Block(3)
        ssa.record_def("i", block1, value1);
        ssa.record_def("i", block3, value3);
        
        // Try to get value at Block(4)
        // Block(4) should get value from Block(3) (its predecessor)
        let result = ssa.get_value_at_end_of_block("i", block4, &function);
        
        // This should succeed and return value3
        assert!(result.is_ok(), "get_value_at_end_of_block should succeed");
        assert_eq!(result.unwrap(), Some(value3), "Block(4) should get value from Block(3)");
    }

    /// Test case: Variable defined in Block(1), read in Block(2) which is successor of Block(1)
    #[test]
    fn test_lazy_dominance_direct_successor() {
        let mut ssa = SSABuilder::new();
        let function = create_test_function();
        
        let block1 = BlockEntity::new(1);
        let block2 = BlockEntity::new(2);
        
        let mut builder = FunctionBuilder::new(Signature::new(vec![], vec![]), "test".to_string());
        let value1 = builder.new_value();
        
        // Record 'i' in Block(1)
        ssa.record_def("i", block1, value1);
        
        // Try to get value at Block(2) which should get it from Block(1)
        let result = ssa.get_value_at_end_of_block("i", block2, &function);
        
        assert!(result.is_ok(), "get_value_at_end_of_block should succeed");
        // Block(2) should get value from Block(1) if Block(1) dominates Block(2)
        // But without a proper CFG, this might not work
        // Let's see what happens
    }

    /// Test case: Reproduce the actual bug from test_nested_control_flow
    /// Block(4) is successor of Block(3), 'i' is defined in Block(1) and Block(3)
    /// Block(4) should get value from Block(3)
    #[test]
    fn test_lazy_dominance_block4_from_block3() {
        let mut ssa = SSABuilder::new();
        
        // Create a function with proper CFG structure
        let sig = Signature::new(vec![], vec![]);
        let mut builder = FunctionBuilder::new(sig, "test".to_string());
        
        // Create blocks in order
        let block0 = builder.create_block();
        let block1 = builder.create_block();
        let _block2 = builder.create_block();
        let block3 = builder.create_block();
        let block4 = builder.create_block();
        
        // Set up CFG: Block(0) -> Block(1) -> Block(3) -> Block(4)
        // Block(1) -> Block(3) (for loop entry -> condition)
        // Block(3) -> Block(4) (condition -> body)
        let mut block0_builder = builder.block_builder(block0);
        block0_builder.jump(block1, &vec![]);
        drop(block0_builder);
        
        let mut block1_builder = builder.block_builder(block1);
        block1_builder.jump(block3, &vec![]);
        drop(block1_builder);
        
        let mut block3_builder = builder.block_builder(block3);
        block3_builder.jump(block4, &vec![]);
        drop(block3_builder);
        
        let function = builder.finish();
        
        // Create values
        let mut value_builder = FunctionBuilder::new(Signature::new(vec![], vec![]), "val_test".to_string());
        let value1 = value_builder.new_value();
        let value3 = value_builder.new_value();
        
        // Record 'i' in Block(1) and Block(3)
        ssa.record_def("i", block1, value1);
        ssa.record_def("i", block3, value3);
        
        // Try to get value at Block(4)
        // Block(4) should get value from Block(3) (its only predecessor)
        let result = ssa.get_value_at_end_of_block("i", block4, &function);
        
        assert!(result.is_ok(), "get_value_at_end_of_block should succeed");
        let value = result.unwrap();
        assert_eq!(value, Some(value3), "Block(4) should get value from Block(3), got {:?}", value);
    }

    /// Test IDom computation: Diamond pattern
    /// Block(0) -> Block(1), Block(0) -> Block(2), Block(1) -> Block(3), Block(2) -> Block(3)
    /// Block(0) defines 'i'
    /// Block(3) should have IDom = Block(0) (intersection of Block(1) and Block(2)'s IDoms)
    #[test]
    fn test_idom_diamond() {
        let mut ssa = SSABuilder::new();
        
        let sig = Signature::new(vec![], vec![]);
        let mut builder = FunctionBuilder::new(sig, "test".to_string());
        
        let block0 = builder.create_block();
        let block1 = builder.create_block();
        let block2 = builder.create_block();
        let block3 = builder.create_block();
        
        // CFG: Block(0) -> Block(1) -> Block(3)
        //      Block(0) -> Block(2) -> Block(3)
        let condition = builder.new_value();
        let mut b0 = builder.block_builder(block0);
        b0.br(condition, block1, &[], block2, &[]); // Branch to both
        drop(b0);
        
        let mut b1 = builder.block_builder(block1);
        b1.jump(block3, &vec![]);
        drop(b1);
        
        let mut b2 = builder.block_builder(block2);
        b2.jump(block3, &vec![]);
        drop(b2);
        
        let function = builder.finish();
        
        let mut value_builder = FunctionBuilder::new(Signature::new(vec![], vec![]), "val_test".to_string());
        let value0 = value_builder.new_value();
        
        // Record 'i' in Block(0)
        ssa.record_def("i", block0, value0);
        
        // Get value at Block(3) - this will trigger dominance computation
        let result = ssa.get_value_at_end_of_block("i", block3, &function);
        assert!(result.is_ok(), "get_value_at_end_of_block should succeed");
        // Block(3) should get value from Block(0) via IDom
        assert_eq!(result.unwrap(), Some(value0), "Block(3) should get value from Block(0)");
    }

    /// Test IDom computation: Loop back-edge (the actual bug case)
    /// CFG: Block(0) -> Block(1) -> Block(3) -> Block(4) -> Block(7) -> Block(9) -> Block(5) -> Block(3)
    /// Block(3) defines 'i'
    /// Block(5) should have IDom = Block(3) (via loop structure)
    #[test]
    fn test_idom_loop_backedge() {
        let mut ssa = SSABuilder::new();
        
        let sig = Signature::new(vec![], vec![]);
        let mut builder = FunctionBuilder::new(sig, "test".to_string());
        
        let block0 = builder.create_block();
        let block1 = builder.create_block();
        let block3 = builder.create_block();
        let block4 = builder.create_block();
        let block5 = builder.create_block();
        let block7 = builder.create_block();
        let block9 = builder.create_block();
        
        // CFG structure matching test_nested_control_flow:
        // Block(0) -> Block(1) -> Block(3) -> Block(4) -> Block(7) -> Block(9) -> Block(5) -> Block(3)
        let mut b0 = builder.block_builder(block0);
        b0.jump(block1, &vec![]);
        drop(b0);
        
        let mut b1 = builder.block_builder(block1);
        b1.jump(block3, &vec![]);
        drop(b1);
        
        let mut b3 = builder.block_builder(block3);
        b3.jump(block4, &vec![]);
        drop(b3);
        
        let mut b4 = builder.block_builder(block4);
        b4.jump(block7, &vec![]);
        drop(b4);
        
        let mut b7 = builder.block_builder(block7);
        b7.jump(block9, &vec![]);
        drop(b7);
        
        let mut b9 = builder.block_builder(block9);
        b9.jump(block5, &vec![]);
        drop(b9);
        
        let mut b5 = builder.block_builder(block5);
        b5.jump(block3, &vec![]); // Loop back-edge
        drop(b5);
        
        let function = builder.finish();
        
        let mut value_builder = FunctionBuilder::new(Signature::new(vec![], vec![]), "val_test".to_string());
        let value3 = value_builder.new_value();
        
        // Record 'i' in Block(3) (loop header)
        ssa.record_def("i", block3, value3);
        
        // Get value at Block(5) - this will trigger dominance computation
        // Block(5) should get value from Block(3) via IDom
        let result = ssa.get_value_at_end_of_block("i", block5, &function);
        assert!(result.is_ok(), "get_value_at_end_of_block should succeed");
        let value = result.unwrap();
        assert_eq!(value, Some(value3), "Block(5) should get value from Block(3) via IDom, got {:?}", value);
    }

    /// Test IDom computation: Multiple predecessors with different IDoms
    /// Block(0) -> Block(1) -> Block(3), Block(0) -> Block(2) -> Block(3)
    /// Block(1) and Block(2) both define 'i' (roots)
    /// Block(3) should have IDom = Block(0) (intersection of Block(1) and Block(2)'s IDoms, which is pseudo-entry)
    #[test]
    fn test_idom_multiple_predecessors() {
        let mut ssa = SSABuilder::new();
        
        let sig = Signature::new(vec![], vec![]);
        let mut builder = FunctionBuilder::new(sig, "test".to_string());
        
        let block0 = builder.create_block();
        let block1 = builder.create_block();
        let block2 = builder.create_block();
        let block3 = builder.create_block();
        
        // CFG: Block(0) -> Block(1) -> Block(3)
        //      Block(0) -> Block(2) -> Block(3)
        let condition = builder.new_value();
        let mut b0 = builder.block_builder(block0);
        b0.br(condition, block1, &[], block2, &[]);
        drop(b0);
        
        let mut b1 = builder.block_builder(block1);
        b1.jump(block3, &vec![]);
        drop(b1);
        
        let mut b2 = builder.block_builder(block2);
        b2.jump(block3, &vec![]);
        drop(b2);
        
        let function = builder.finish();
        
        let mut value_builder = FunctionBuilder::new(Signature::new(vec![], vec![]), "val_test".to_string());
        let value1 = value_builder.new_value();
        let value2 = value_builder.new_value();
        
        // Record 'i' in Block(1) and Block(2) (both are roots)
        ssa.record_def("i", block1, value1);
        ssa.record_def("i", block2, value2);
        
        // Get value at Block(3) - this will trigger dominance computation
        // Block(3) should get value via PHI from Block(1) and Block(2)
        let result = ssa.get_value_at_end_of_block("i", block3, &function);
        assert!(result.is_ok(), "get_value_at_end_of_block should succeed");
        // Should get one of the values (PHI will be created)
        let value = result.unwrap();
        assert!(value.is_some(), "Block(3) should get a value (via PHI)");
    }

    /// Test IDom computation: Linear chain
    /// Block(0) -> Block(1) -> Block(2)
    /// Block(0) defines 'i'
    /// Block(1) should have IDom = Block(0), Block(2) should have IDom = Block(1)
    #[test]
    fn test_idom_linear_chain() {
        let mut ssa = SSABuilder::new();
        
        let sig = Signature::new(vec![], vec![]);
        let mut builder = FunctionBuilder::new(sig, "test".to_string());
        
        let block0 = builder.create_block();
        let block1 = builder.create_block();
        let block2 = builder.create_block();
        
        // CFG: Block(0) -> Block(1) -> Block(2)
        let mut b0 = builder.block_builder(block0);
        b0.jump(block1, &vec![]);
        drop(b0);
        
        let mut b1 = builder.block_builder(block1);
        b1.jump(block2, &vec![]);
        drop(b1);
        
        let function = builder.finish();
        
        let mut value_builder = FunctionBuilder::new(Signature::new(vec![], vec![]), "val_test".to_string());
        let value0 = value_builder.new_value();
        
        // Record 'i' in Block(0)
        ssa.record_def("i", block0, value0);
        
        // Get value at Block(2) - this will trigger dominance computation
        // Block(2) should get value from Block(0) via IDom chain
        let result = ssa.get_value_at_end_of_block("i", block2, &function);
        assert!(result.is_ok(), "get_value_at_end_of_block should succeed");
        assert_eq!(result.unwrap(), Some(value0), "Block(2) should get value from Block(0)");
    }

    /// Test IDom computation: Single block function
    /// Block(0) defines 'i' and uses it
    #[test]
    fn test_idom_single_block() {
        let mut ssa = SSABuilder::new();
        
        let sig = Signature::new(vec![], vec![]);
        let mut builder = FunctionBuilder::new(sig, "test".to_string());
        
        let block0 = builder.create_block();
        let function = builder.finish();
        
        let mut value_builder = FunctionBuilder::new(Signature::new(vec![], vec![]), "val_test".to_string());
        let value0 = value_builder.new_value();
        
        // Record 'i' in Block(0)
        ssa.record_def("i", block0, value0);
        
        // Get value at Block(0) - should return the value directly
        let result = ssa.get_value_at_end_of_block("i", block0, &function);
        assert!(result.is_ok(), "get_value_at_end_of_block should succeed");
        assert_eq!(result.unwrap(), Some(value0), "Block(0) should get its own value");
    }

    /// Test IDom computation: Function with no definitions
    /// Should return None
    #[test]
    fn test_idom_no_definitions() {
        let mut ssa = SSABuilder::new();
        
        let sig = Signature::new(vec![], vec![]);
        let mut builder = FunctionBuilder::new(sig, "test".to_string());
        
        let block0 = builder.create_block();
        let block1 = builder.create_block();
        
        let mut b0 = builder.block_builder(block0);
        b0.jump(block1, &vec![]);
        drop(b0);
        
        let function = builder.finish();
        
        // Don't record any definitions
        // Get value at Block(1) - should return None
        let result = ssa.get_value_at_end_of_block("i", block1, &function);
        assert!(result.is_ok(), "get_value_at_end_of_block should succeed");
        assert_eq!(result.unwrap(), None, "Block(1) should have no value");
    }

    /// Test IDom computation: Block with self-loop
    /// Block(0) -> Block(1) -> Block(0) (self-loop)
    /// Block(0) defines 'i'
    #[test]
    fn test_idom_self_loop() {
        let mut ssa = SSABuilder::new();
        
        let sig = Signature::new(vec![], vec![]);
        let mut builder = FunctionBuilder::new(sig, "test".to_string());
        
        let block0 = builder.create_block();
        let block1 = builder.create_block();
        
        // CFG: Block(0) -> Block(1) -> Block(0)
        let mut b0 = builder.block_builder(block0);
        b0.jump(block1, &vec![]);
        drop(b0);
        
        let mut b1 = builder.block_builder(block1);
        b1.jump(block0, &vec![]);
        drop(b1);
        
        let function = builder.finish();
        
        let mut value_builder = FunctionBuilder::new(Signature::new(vec![], vec![]), "val_test".to_string());
        let value0 = value_builder.new_value();
        
        // Record 'i' in Block(0)
        ssa.record_def("i", block0, value0);
        
        // Get value at Block(1) - should get value from Block(0) via IDom
        let result = ssa.get_value_at_end_of_block("i", block1, &function);
        assert!(result.is_ok(), "get_value_at_end_of_block should succeed");
        let value = result.unwrap();
        assert_eq!(value, Some(value0), "Block(1) should get value from Block(0)");
    }

    /// Test IDom computation: Multiple roots with different postorder numbers
    /// Block(0) -> Block(1) -> Block(3), Block(0) -> Block(2) -> Block(3)
    /// Block(1) and Block(2) both define 'i' (roots)
    /// Block(3) should get value via PHI
    #[test]
    fn test_idom_multiple_roots() {
        let mut ssa = SSABuilder::new();
        
        let sig = Signature::new(vec![], vec![]);
        let mut builder = FunctionBuilder::new(sig, "test".to_string());
        
        let block0 = builder.create_block();
        let block1 = builder.create_block();
        let block2 = builder.create_block();
        let block3 = builder.create_block();
        
        // CFG: Block(0) -> Block(1) -> Block(3)
        //      Block(0) -> Block(2) -> Block(3)
        let condition = builder.new_value();
        let mut b0 = builder.block_builder(block0);
        b0.br(condition, block1, &[], block2, &[]);
        drop(b0);
        
        let mut b1 = builder.block_builder(block1);
        b1.jump(block3, &vec![]);
        drop(b1);
        
        let mut b2 = builder.block_builder(block2);
        b2.jump(block3, &vec![]);
        drop(b2);
        
        let function = builder.finish();
        
        let mut value_builder = FunctionBuilder::new(Signature::new(vec![], vec![]), "val_test".to_string());
        let value1 = value_builder.new_value();
        let value2 = value_builder.new_value();
        
        // Record 'i' in Block(1) and Block(2) (both are roots)
        ssa.record_def("i", block1, value1);
        ssa.record_def("i", block2, value2);
        
        // Get value at Block(3) - should get value via PHI
        let result = ssa.get_value_at_end_of_block("i", block3, &function);
        assert!(result.is_ok(), "get_value_at_end_of_block should succeed");
        let value = result.unwrap();
        assert!(value.is_some(), "Block(3) should get a value (via PHI)");
        // Should get one of the two values
        assert!(value == Some(value1) || value == Some(value2), 
                "Block(3) should get value1 or value2");
    }

    /// Test IDom computation: Entry block (no predecessors)
    /// Block(0) defines 'i' and has no predecessors
    #[test]
    fn test_idom_entry_block() {
        let mut ssa = SSABuilder::new();
        
        let sig = Signature::new(vec![], vec![]);
        let mut builder = FunctionBuilder::new(sig, "test".to_string());
        
        let block0 = builder.create_block();
        let block1 = builder.create_block();
        
        // CFG: Block(0) -> Block(1)
        let mut b0 = builder.block_builder(block0);
        b0.jump(block1, &vec![]);
        drop(b0);
        
        let function = builder.finish();
        
        let mut value_builder = FunctionBuilder::new(Signature::new(vec![], vec![]), "val_test".to_string());
        let value0 = value_builder.new_value();
        
        // Record 'i' in Block(0) (entry block)
        ssa.record_def("i", block0, value0);
        
        // Get value at Block(0) - should return the value directly
        let result = ssa.get_value_at_end_of_block("i", block0, &function);
        assert!(result.is_ok(), "get_value_at_end_of_block should succeed");
        assert_eq!(result.unwrap(), Some(value0), "Block(0) should get its own value");
        
        // Get value at Block(1) - should get value from Block(0) via IDom
        let result = ssa.get_value_at_end_of_block("i", block1, &function);
        assert!(result.is_ok(), "get_value_at_end_of_block should succeed");
        assert_eq!(result.unwrap(), Some(value0), "Block(1) should get value from Block(0)");
    }

    /// Test IDom computation: Exit block (no successors)
    /// Block(0) -> Block(1) (exit)
    /// Block(0) defines 'i'
    #[test]
    fn test_idom_exit_block() {
        let mut ssa = SSABuilder::new();
        
        let sig = Signature::new(vec![], vec![]);
        let mut builder = FunctionBuilder::new(sig, "test".to_string());
        
        let block0 = builder.create_block();
        let block1 = builder.create_block();
        
        // CFG: Block(0) -> Block(1) (exit, no successors)
        let mut b0 = builder.block_builder(block0);
        b0.jump(block1, &vec![]);
        drop(b0);
        
        let function = builder.finish();
        
        let mut value_builder = FunctionBuilder::new(Signature::new(vec![], vec![]), "val_test".to_string());
        let value0 = value_builder.new_value();
        
        // Record 'i' in Block(0)
        ssa.record_def("i", block0, value0);
        
        // Get value at Block(1) - should get value from Block(0) via IDom
        let result = ssa.get_value_at_end_of_block("i", block1, &function);
        assert!(result.is_ok(), "get_value_at_end_of_block should succeed");
        assert_eq!(result.unwrap(), Some(value0), "Block(1) should get value from Block(0)");
    }

    /// Test IDom computation: Unreachable predecessor handling
    /// Block(0) -> Block(1), Block(2) -> Block(1) (Block(2) is unreachable)
    /// Block(0) defines 'i'
    /// Block(1) should get value from Block(0) (not Block(2))
    #[test]
    fn test_idom_unreachable_predecessor() {
        let mut ssa = SSABuilder::new();
        
        let sig = Signature::new(vec![], vec![]);
        let mut builder = FunctionBuilder::new(sig, "test".to_string());
        
        let block0 = builder.create_block();
        let block1 = builder.create_block();
        let block2 = builder.create_block();
        
        // CFG: Block(0) -> Block(1)
        //      Block(2) -> Block(1) (but Block(2) is unreachable from entry)
        let mut b0 = builder.block_builder(block0);
        b0.jump(block1, &vec![]);
        drop(b0);
        
        // Block(2) is unreachable - no path from entry to it
        // But we can still create an edge from Block(2) to Block(1)
        let mut b2 = builder.block_builder(block2);
        b2.jump(block1, &vec![]);
        drop(b2);
        
        let function = builder.finish();
        
        let mut value_builder = FunctionBuilder::new(Signature::new(vec![], vec![]), "val_test".to_string());
        let value0 = value_builder.new_value();
        
        // Record 'i' in Block(0)
        ssa.record_def("i", block0, value0);
        
        // Get value at Block(1) - should get value from Block(0) (reachable predecessor)
        // Block(2) is unreachable, so it shouldn't affect the result
        let result = ssa.get_value_at_end_of_block("i", block1, &function);
        assert!(result.is_ok(), "get_value_at_end_of_block should succeed");
        let value = result.unwrap();
        assert_eq!(value, Some(value0), "Block(1) should get value from Block(0), not unreachable Block(2)");
    }

    /// Test IDom computation: Loop with multiple back-edges
    /// Block(0) -> Block(1) -> Block(2) -> Block(1) (loop)
    /// Block(1) defines 'i'
    /// Block(2) should get value from Block(1) via IDom
    #[test]
    fn test_idom_loop_multiple_backedges() {
        let mut ssa = SSABuilder::new();
        
        let sig = Signature::new(vec![], vec![]);
        let mut builder = FunctionBuilder::new(sig, "test".to_string());
        
        let block0 = builder.create_block();
        let block1 = builder.create_block();
        let block2 = builder.create_block();
        
        // CFG: Block(0) -> Block(1) -> Block(2) -> Block(1) (loop)
        let mut b0 = builder.block_builder(block0);
        b0.jump(block1, &vec![]);
        drop(b0);
        
        let condition = builder.new_value();
        let mut b1 = builder.block_builder(block1);
        b1.br(condition, block2, &[], block1, &[]); // Can branch to self or Block(2)
        drop(b1);
        
        let mut b2 = builder.block_builder(block2);
        b2.jump(block1, &vec![]);
        drop(b2);
        
        let function = builder.finish();
        
        let mut value_builder = FunctionBuilder::new(Signature::new(vec![], vec![]), "val_test".to_string());
        let value1 = value_builder.new_value();
        
        // Record 'i' in Block(1) (loop header)
        ssa.record_def("i", block1, value1);
        
        // Get value at Block(2) - should get value from Block(1) via IDom
        let result = ssa.get_value_at_end_of_block("i", block2, &function);
        assert!(result.is_ok(), "get_value_at_end_of_block should succeed");
        let value = result.unwrap();
        assert_eq!(value, Some(value1), "Block(2) should get value from Block(1) via IDom");
    }
}

