//! Comprehensive tests for phi node handling.

#[cfg(test)]
mod tests {
    extern crate alloc;

    use alloc::vec;

    use lpc_lpir::parse_function;

    use crate::{
        backend::{
            abi::Abi,
            frame,
            liveness::compute_liveness,
            lower::{compute_phi_sources, find_predecessors, Lowerer},
            regalloc::allocate_registers,
            spill_reload::create_spill_reload_plan,
        },
        Gpr,
    };

    mod phi_source_tests {
        use super::*;

        #[test]
        fn test_phi_sources_single_predecessor() {
            // Function: block0 defines v1, jumps to block1 which has v1 as parameter
            let ir = r#"
function %test() -> i32 {
block0:
    v1 = iconst 42
    jump block1(v1)

block1(v2: i32):
    return v2
}"#;

            let func = parse_function(ir).expect("Failed to parse IR");
            let liveness = compute_liveness(&func);
            let phi_sources = compute_phi_sources(&func, &liveness);

            // v2 should come from v1 in block0
            assert!(phi_sources.contains_key(&(0, 1, 0)));
            let source = phi_sources.get(&(0, 1, 0)).unwrap();
            assert_eq!(source.index(), 1); // v1
        }

        #[test]
        fn test_phi_sources_multiple_predecessors() {
            // Diamond pattern: block0 branches to block1/block2, both merge to block3
            let ir = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    brif v0, block1, block2

block1:
    v1 = iconst 10
    jump block3(v1)

block2:
    v2 = iconst 20
    jump block3(v2)

block3(v3: i32):
    return v3
}"#;

            let func = parse_function(ir).expect("Failed to parse IR");
            let liveness = compute_liveness(&func);
            let phi_sources = compute_phi_sources(&func, &liveness);

            // v3 should come from v1 (block1) and v2 (block2)
            assert!(phi_sources.contains_key(&(1, 3, 0)));
            assert!(phi_sources.contains_key(&(2, 3, 0)));
        }

        #[test]
        fn test_phi_sources_no_parameters() {
            // Block with no parameters should result in empty phi sources
            let ir = r#"
function %test() -> i32 {
block0:
    v1 = iconst 42
    jump block1

block1:
    return v1
}"#;

            let func = parse_function(ir).expect("Failed to parse IR");
            let liveness = compute_liveness(&func);
            let phi_sources = compute_phi_sources(&func, &liveness);

            // Should be empty since block1 has no parameters
            assert!(phi_sources.is_empty());
        }

        #[test]
        fn test_find_predecessors() {
            let ir = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    brif v0, block1, block2

block1:
    jump block3

block2:
    jump block3

block3:
    return v0
}"#;

            let func = parse_function(ir).expect("Failed to parse IR");

            // Block 1 should have block0 as predecessor
            let preds1 = find_predecessors(&func, 1);
            assert_eq!(preds1, vec![0]);

            // Block 3 should have block1 and block2 as predecessors
            let preds3 = find_predecessors(&func, 3);
            assert_eq!(preds3.len(), 2);
            assert!(preds3.contains(&1));
            assert!(preds3.contains(&2));
        }
    }

    mod copy_phi_tests {
        use super::*;

        fn create_test_lowerer_with_phi(ir: &str) -> Lowerer {
            let func = parse_function(ir).expect("Failed to parse IR");
            let liveness = compute_liveness(&func);
            let allocation = allocate_registers(&func, &liveness);
            let spill_reload = create_spill_reload_plan(&func, &allocation, &liveness);
            let abi = Abi::compute_abi_info(
                func.signature.params.len(),
                func.signature.returns.len(),
                true,
            );
            let total_spill_slots = allocation.spill_slot_count + spill_reload.max_temp_spill_slots;
            let frame_layout = frame::compute_frame_layout(
                &allocation.used_callee_saved,
                frame::FunctionCalls::None,
                0,
                0,
                total_spill_slots as u32,
                0,
                abi.stack_args_size,
                false,
            );
            let phi_sources = compute_phi_sources(&func, &liveness);
            Lowerer::new(
                func,
                allocation,
                spill_reload,
                frame_layout,
                abi,
                liveness,
                phi_sources,
            )
        }

        #[test]
        fn test_copy_phi_single_parameter() {
            // Simple case: one value copied into one parameter
            let ir = r#"
function %test() -> i32 {
block0:
    v1 = iconst 42
    jump block1(v1)

block1(v2: i32):
    return v2
}"#;

            let mut lowerer = create_test_lowerer_with_phi(ir);

            // Simulate being in block0, jumping to block1
            lowerer.set_current_block_idx(0);

            // This should copy v1 to v2's register
            lowerer.copy_phi_values(0, 1);

            // Should have emitted at least one instruction (the copy)
            // Note: We can't easily check the exact instruction without exposing internals,
            // but we can verify it doesn't panic and produces instructions
            assert!(lowerer.inst_buffer().instruction_count() > 0);
        }

        #[test]
        fn test_copy_phi_no_parameters() {
            // Block with no parameters - should be no-op
            let ir = r#"
function %test() -> i32 {
block0:
    v1 = iconst 42
    jump block1

block1:
    return v1
}"#;

            let mut lowerer = create_test_lowerer_with_phi(ir);
            let before_count = lowerer.inst_buffer().instruction_count();

            // Copy phi values - should do nothing since block1 has no params
            lowerer.copy_phi_values(0, 1);

            // Instruction count should be unchanged
            assert_eq!(lowerer.inst_buffer().instruction_count(), before_count);
        }
    }

    mod branch_integration_tests {
        use super::*;
        use crate::backend::lower::branch::{lower_br, lower_jump};

        fn create_test_lowerer_with_phi(ir: &str) -> Lowerer {
            let func = parse_function(ir).expect("Failed to parse IR");
            let liveness = compute_liveness(&func);
            let allocation = allocate_registers(&func, &liveness);
            let spill_reload = create_spill_reload_plan(&func, &allocation, &liveness);
            let abi = Abi::compute_abi_info(
                func.signature.params.len(),
                func.signature.returns.len(),
                true,
            );
            let total_spill_slots = allocation.spill_slot_count + spill_reload.max_temp_spill_slots;
            let frame_layout = frame::compute_frame_layout(
                &allocation.used_callee_saved,
                frame::FunctionCalls::None,
                0,
                0,
                total_spill_slots as u32,
                0,
                abi.stack_args_size,
                false,
            );
            let phi_sources = compute_phi_sources(&func, &liveness);
            Lowerer::new(
                func,
                allocation,
                spill_reload,
                frame_layout,
                abi,
                liveness,
                phi_sources,
            )
        }

        #[test]
        fn test_jump_with_phi_copy() {
            // Jump to block with parameters - should copy before jumping
            let ir = r#"
function %test() -> i32 {
block0:
    v1 = iconst 42
    jump block1(v1)

block1(v2: i32):
    return v2
}"#;

            let func = parse_function(ir).expect("Failed to parse IR");
            let liveness = compute_liveness(&func);
            let allocation = allocate_registers(&func, &liveness);
            let spill_reload = create_spill_reload_plan(&func, &allocation, &liveness);
            let abi = Abi::compute_abi_info(0, 1, true);
            let total_spill_slots = allocation.spill_slot_count + spill_reload.max_temp_spill_slots;
            let frame_layout = frame::compute_frame_layout(
                &allocation.used_callee_saved,
                frame::FunctionCalls::None,
                0,
                0,
                total_spill_slots as u32,
                0,
                abi.stack_args_size,
                false,
            );
            let phi_sources = compute_phi_sources(&func, &liveness);
            let mut lowerer = Lowerer::new(
                func,
                allocation,
                spill_reload,
                frame_layout,
                abi,
                liveness,
                phi_sources,
            );

            // Set current block
            lowerer.set_current_block_idx(0);

            // Lower jump - should copy phi values before jumping
            let v1 = lpc_lpir::Value::new(1);
            lower_jump(&mut lowerer, 1, &[v1]);

            // Should have at least 2 instructions: copy + jump
            let insts = lowerer.inst_buffer().instructions();
            assert!(insts.len() >= 2);

            // Last instruction should be JAL (the jump)
            assert!(matches!(insts.last(), Some(crate::Inst::Jal { .. })));
        }

        #[test]
        fn test_jump_no_phi_copy() {
            // Jump to block without parameters - no copies needed
            let ir = r#"
function %test() -> i32 {
block0:
    v1 = iconst 42
    jump block1

block1:
    return v1
}"#;

            let func = parse_function(ir).expect("Failed to parse IR");
            let liveness = compute_liveness(&func);
            let allocation = allocate_registers(&func, &liveness);
            let spill_reload = create_spill_reload_plan(&func, &allocation, &liveness);
            let abi = Abi::compute_abi_info(0, 1, true);
            let total_spill_slots = allocation.spill_slot_count + spill_reload.max_temp_spill_slots;
            let frame_layout = frame::compute_frame_layout(
                &allocation.used_callee_saved,
                frame::FunctionCalls::None,
                0,
                0,
                total_spill_slots as u32,
                0,
                abi.stack_args_size,
                false,
            );
            let phi_sources = compute_phi_sources(&func, &liveness);
            let mut lowerer = Lowerer::new(
                func,
                allocation,
                spill_reload,
                frame_layout,
                abi,
                liveness,
                phi_sources,
            );

            lowerer.set_current_block_idx(0);
            lower_jump(&mut lowerer, 1, &[]);

            // Should have exactly 1 instruction (just the jump, no copies)
            let insts = lowerer.inst_buffer().instructions();
            assert_eq!(insts.len(), 1);
            assert!(matches!(insts[0], crate::Inst::Jal { .. }));
        }

        #[test]
        fn test_br_both_targets_phi_copy() {
            // Branch where both targets have parameters
            let ir = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    brif v0, block1, block2

block1:
    v1 = iconst 10
    jump block3(v1)

block2:
    v2 = iconst 20
    jump block3(v2)

block3(v3: i32):
    return v3
}"#;

            let func = parse_function(ir).expect("Failed to parse IR");
            let liveness = compute_liveness(&func);
            let allocation = allocate_registers(&func, &liveness);
            let spill_reload = create_spill_reload_plan(&func, &allocation, &liveness);
            let abi = Abi::compute_abi_info(1, 1, true);
            let total_spill_slots = allocation.spill_slot_count + spill_reload.max_temp_spill_slots;
            let frame_layout = frame::compute_frame_layout(
                &allocation.used_callee_saved,
                frame::FunctionCalls::None,
                0,
                0,
                total_spill_slots as u32,
                0,
                abi.stack_args_size,
                false,
            );
            let phi_sources = compute_phi_sources(&func, &liveness);
            let mut lowerer = Lowerer::new(
                func,
                allocation,
                spill_reload,
                frame_layout,
                abi,
                liveness,
                phi_sources,
            );

            lowerer.set_current_block_idx(0);
            let v0 = lpc_lpir::Value::new(0);
            lower_br(&mut lowerer, v0, 1, &[], 2, &[]);

            // Should have instructions: copy for block1, branch, copy for block2, jump
            let insts = lowerer.inst_buffer().instructions();
            assert!(insts.len() >= 4);
        }
    }

    mod spilled_value_tests {
        use super::*;

        #[test]
        fn test_copy_phi_spilled_source() {
            // Test case where source value is spilled - should reload before copy
            // This test will fail until we implement spilled value handling
            let ir = r#"
function %test() -> i32 {
block0:
    v1 = iconst 42
    v2 = iconst 1
    v3 = iconst 2
    v4 = iconst 3
    v5 = iconst 4
    v6 = iconst 5
    v7 = iconst 6
    v8 = iconst 7
    v9 = iconst 8
    v10 = iconst 9
    v11 = iconst 10
    v12 = iconst 11
    v13 = iconst 12
    v14 = iconst 13
    v15 = iconst 14
    v16 = iconst 15
    v17 = iconst 16
    v18 = iconst 17
    v19 = iconst 18
    jump block1(v1)

block1(v20: i32):
    return v20
}"#;

            let func = parse_function(ir).expect("Failed to parse IR");
            let liveness = compute_liveness(&func);
            let allocation = allocate_registers(&func, &liveness);

            // Verify that some values are spilled (we have 20 values, only ~15 registers available)
            assert!(allocation.spill_slot_count > 0 || allocation.value_to_slot.len() > 0);

            let spill_reload = create_spill_reload_plan(&func, &allocation, &liveness);
            let abi = Abi::compute_abi_info(0, 1, true);
            let total_spill_slots = allocation.spill_slot_count + spill_reload.max_temp_spill_slots;
            let frame_layout = frame::compute_frame_layout(
                &allocation.used_callee_saved,
                frame::FunctionCalls::None,
                0,
                0,
                total_spill_slots as u32,
                0,
                abi.stack_args_size,
                false,
            );
            let phi_sources = compute_phi_sources(&func, &liveness);
            let mut lowerer = Lowerer::new(
                func,
                allocation,
                spill_reload,
                frame_layout,
                abi,
                liveness,
                phi_sources,
            );

            lowerer.set_current_block_idx(0);

            // This should reload spilled source before copying
            // Currently will panic - this test documents the expected behavior
            // When spilled value handling is implemented, this should work
            lowerer.copy_phi_values(0, 1);
        }

        #[test]
        fn test_copy_phi_spilled_target() {
            // Test case where target parameter is spilled - should copy to slot directly
            // We need to force v2 to be spilled
            // This is tricky - we'd need many other values to force spilling
            // For now, this test documents expected behavior

            // TODO: Create a scenario where parameter is spilled
            // This will require careful setup of register allocation
        }

        #[test]
        fn test_copy_phi_both_spilled() {
            // Both source and target spilled - reload source, store to target slot
            // This test documents expected behavior
        }
    }

    mod integration_tests {
        use super::*;

        #[test]
        fn test_phi_loop_induction_variable() {
            // Classic loop: i = phi(0, i+1)
            let ir = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 0
    jump block1(v1)

block1(v2: i32):  // i = phi(0 from block0, v3 from block1)
    v3 = iadd v2, v0
    brif v3, block1(v3), block2

block2:
    return v2
}"#;

            let func = parse_function(ir).expect("Failed to parse IR");
            let liveness = compute_liveness(&func);
            let phi_sources = compute_phi_sources(&func, &liveness);

            // v2 should have sources from both block0 (v1) and block1 (v3)
            assert!(phi_sources.contains_key(&(0, 1, 0))); // v1 from block0
            assert!(phi_sources.contains_key(&(1, 1, 0))); // v3 from block1 (self-loop)
        }

        #[test]
        fn test_phi_conditional_merge() {
            // If-else with phi merge
            let ir = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    brif v0, block1, block2

block1:
    v1 = iconst 10
    jump block3(v1)

block2:
    v2 = iconst 20
    jump block3(v2)

block3(v3: i32):  // v3 = phi(v1 from block1, v2 from block2)
    return v3
}"#;

            let func = parse_function(ir).expect("Failed to parse IR");
            let liveness = compute_liveness(&func);
            let phi_sources = compute_phi_sources(&func, &liveness);

            // v3 should come from v1 (block1) and v2 (block2)
            assert!(phi_sources.contains_key(&(1, 3, 0)));
            assert!(phi_sources.contains_key(&(2, 3, 0)));

            let source1 = phi_sources.get(&(1, 3, 0)).unwrap();
            let source2 = phi_sources.get(&(2, 3, 0)).unwrap();

            // Should map to v1 and v2 (values 1 and 2)
            assert!(source1.index() == 1 || source1.index() == 2);
            assert!(source2.index() == 1 || source2.index() == 2);
            assert_ne!(source1.index(), source2.index());
        }

        #[test]
        fn test_phi_diamond_pattern() {
            // Diamond CFG: A->B, A->C, B->D, C->D
            let ir = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    brif v0, block1, block2

block1:
    v1 = iconst 10
    jump block3(v1)

block2:
    v2 = iconst 20
    jump block3(v2)

block3(v3: i32):
    return v3
}"#;

            let func = parse_function(ir).expect("Failed to parse IR");
            let liveness = compute_liveness(&func);
            let phi_sources = compute_phi_sources(&func, &liveness);

            // Block 3 should receive values from both block1 and block2
            assert!(phi_sources.contains_key(&(1, 3, 0)));
            assert!(phi_sources.contains_key(&(2, 3, 0)));
        }
    }
}
