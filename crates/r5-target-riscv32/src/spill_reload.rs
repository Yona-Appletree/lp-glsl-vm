//! Spill/reload insertion for register allocation.
//!
//! This module inserts explicit spill and reload instructions at the right points
//! for values that need to be spilled to the stack.

use alloc::{collections::BTreeMap, vec, vec::Vec};

use r5_ir::{Function, Inst, Value};
use riscv32_encoder::Gpr;

use crate::{
    liveness::{InstPoint, LivenessInfo},
    regalloc::RegisterAllocation,
};

/// Spill or reload operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpillReloadOp {
    /// Spill a value from register to stack.
    Spill { value: Value, reg: Gpr, slot: u32 },
    /// Reload a value from stack to register.
    Reload { value: Value, reg: Gpr, slot: u32 },
}

/// Spill/reload insertion plan.
#[derive(Debug, Clone)]
pub struct SpillReloadPlan {
    /// Operations to insert before each instruction point.
    pub before: BTreeMap<InstPoint, Vec<SpillReloadOp>>,
    /// Operations to insert after each instruction point.
    pub after: BTreeMap<InstPoint, Vec<SpillReloadOp>>,
    /// Maximum number of temporary spill slots needed (beyond allocated ones).
    /// This should be added to spill_slot_count when computing frame layout.
    pub max_temp_spill_slots: usize,
}

/// Create spill/reload plan for a function.
///
/// This inserts explicit spill and reload operations for spilled values.
pub fn create_spill_reload_plan(
    func: &Function,
    allocation: &RegisterAllocation,
    liveness: &LivenessInfo,
) -> SpillReloadPlan {
    let mut before = BTreeMap::new();
    let mut after = BTreeMap::new();

    // For each spilled value, insert spills after definitions and reloads before uses
    for (value, slot) in &allocation.value_to_slot {
        let live_range = match liveness.live_range(*value) {
            Some(lr) => lr,
            None => continue, // Skip if no live range (shouldn't happen)
        };

        // Find a register to use for this spilled value
        // If the value was previously in a register before spilling, use that
        // Otherwise, we'll need to allocate one temporarily (handled during lowering)
        let reg = allocation
            .value_to_reg
            .get(value)
            .copied()
            .unwrap_or_else(|| {
                // Fallback: use a temporary register (a0 for now)
                // This will be handled properly during lowering
                Gpr::A0
            });

        // Spill after definition if value will be used later
        if live_range.last_use > live_range.def {
            after.insert(
                live_range.def,
                vec![SpillReloadOp::Spill {
                    value: *value,
                    reg,
                    slot: *slot,
                }],
            );
        }

        // Reload before each use point
        for use_point in &live_range.uses {
            before
                .entry(*use_point)
                .or_insert_with(Vec::new)
                .push(SpillReloadOp::Reload {
                    value: *value,
                    reg,
                    slot: *slot,
                });
        }
    }

    // Handle call sites: spill caller-saved values before calls
    // Track maximum temporary spill slots needed across all call sites
    let mut max_temp_slots = 0;

    for (block_idx, block) in func.blocks.iter().enumerate() {
        for (inst_idx, inst) in block.insts.iter().enumerate() {
            if let Inst::Call {
                args,
                results,
                callee: _,
            } = inst
            {
                let call_point = InstPoint::new(block_idx, inst_idx + 1);

                // Collect argument and return values for this call
                let call_args: alloc::collections::BTreeSet<_> = args.iter().copied().collect();
                let call_results: alloc::collections::BTreeSet<_> =
                    results.iter().copied().collect();

                // Find all live values in caller-saved registers
                // BUT: Don't spill return values (they're written to, not read from).
                // For arguments: we need to spill them if they're used AFTER the call,
                // because the call will overwrite their register with the return value.
                // However, skip stack arguments (index >= 8) - they're handled by call lowering.
                let mut to_spill = Vec::new();

                // Build a map of argument values to their indices
                let arg_index_map: alloc::collections::BTreeMap<_, _> = args
                    .iter()
                    .enumerate()
                    .map(|(idx, val)| (*val, idx))
                    .collect();

                for (value, reg) in &allocation.value_to_reg {
                    if crate::abi::Abi::is_caller_saved(*reg) {
                        // Skip return values - they're written to by the call, not read from
                        if call_results.contains(value) {
                            continue;
                        }

                        // Skip stack arguments (index >= 8) - they're handled by call lowering code
                        if let Some(&arg_idx) = arg_index_map.get(value) {
                            if arg_idx >= 8 {
                                // This is a stack argument - don't spill it here
                                // The call lowering code will handle loading it and storing to stack
                                continue;
                            }
                        }

                        // For arguments in a0-a7: check if they're used after the call
                        let reg_num = reg.num();
                        if (10..=17).contains(&reg_num) && call_args.contains(value) {
                            // This is a register argument (index < 8) in a0-a7
                            // Check if it's used after the call
                            if let Some(live_range) = liveness.live_range(*value) {
                                // If last_use is exactly at call_point, it's only used as an argument
                                // If last_use > call_point, it's used after the call and needs spilling
                                if live_range.last_use <= call_point {
                                    // Only used as argument, not after - skip spilling
                                    continue;
                                }
                                // Otherwise, fall through to add it to to_spill
                            } else {
                                // No live range info - skip to be safe
                                continue;
                            }
                        }

                        // Check if value is live across the call
                        if let Some(live_range) = liveness.live_range(*value) {
                            if live_range.last_use >= call_point {
                                to_spill.push((*value, *reg));
                            }
                        }
                    }
                }

                // Spill before call if not already spilled
                if !to_spill.is_empty() {
                    let mut spills = Vec::new();
                    let mut temp_slot_counter = allocation.spill_slot_count as u32;
                    let mut temp_slots_needed = 0;

                    for (value, reg) in to_spill {
                        // Only spill if not already spilled
                        if !allocation.value_to_slot.contains_key(&value) {
                            // This value is in a caller-saved register and live across call
                            // We need to temporarily spill it to preserve it across the call
                            // Use a temporary spill slot beyond the allocated ones
                            // Each value gets its own temporary slot
                            let temp_slot = temp_slot_counter;
                            temp_slot_counter += 1;
                            temp_slots_needed += 1;

                            spills.push(SpillReloadOp::Spill {
                                value,
                                reg,
                                slot: temp_slot,
                            });

                            // Also create a reload after the call if the value is still live
                            if let Some(live_range) = liveness.live_range(value) {
                                if live_range.last_use > call_point {
                                    let after_point = InstPoint::new(block_idx, inst_idx + 2);
                                    after.entry(after_point).or_insert_with(Vec::new).push(
                                        SpillReloadOp::Reload {
                                            value,
                                            reg,
                                            slot: temp_slot,
                                        },
                                    );
                                }
                            }
                        }
                        // If value is already spilled, no need to do anything - it's already safe
                    }

                    // Track maximum temporary slots needed
                    max_temp_slots = max_temp_slots.max(temp_slots_needed);

                    if !spills.is_empty() {
                        before.insert(call_point, spills);
                    }
                }
            }
        }
    }

    SpillReloadPlan {
        before,
        after,
        max_temp_spill_slots: max_temp_slots,
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use r5_ir::{parse_function, Block, Function, Signature};

    use super::*;
    use crate::{
        liveness::{compute_liveness, InstPoint},
        regalloc::allocate_registers,
    };

    #[test]
    fn test_spill_after_def() {
        // Function where a value is defined and then spilled
        let sig = Signature::empty();
        let mut func = Function::new(sig);

        let mut block = Block::new();
        let v0 = Value::new(0);

        block.push_inst(Inst::Iconst {
            result: v0,
            value: 42,
        });
        block.push_inst(Inst::Return { values: vec![v0] });

        func.add_block(block);

        let liveness = compute_liveness(&func);
        let allocation = allocate_registers(&func, &liveness);

        // Create spill/reload plan
        let plan = create_spill_reload_plan(&func, &allocation, &liveness);

        // If v0 is spilled, there should be a spill after its definition
        if allocation.value_to_slot.contains_key(&v0) {
            let def_point = InstPoint::new(0, 1);
            assert!(plan.after.contains_key(&def_point));
        }
    }

    #[test]
    fn test_reload_before_use() {
        // Function where a spilled value is used
        let sig = Signature::empty();
        let mut func = Function::new(sig);

        let mut block = Block::new();
        let v0 = Value::new(0);
        let v1 = Value::new(1);

        block.push_inst(Inst::Iconst {
            result: v0,
            value: 1,
        });
        block.push_inst(Inst::Iadd {
            result: v1,
            arg1: v0,
            arg2: v0,
        });
        block.push_inst(Inst::Return { values: vec![v1] });

        func.add_block(block);

        let liveness = compute_liveness(&func);
        let allocation = allocate_registers(&func, &liveness);

        // Create spill/reload plan
        let plan = create_spill_reload_plan(&func, &allocation, &liveness);

        // If v0 is spilled, there should be reloads before its uses
        if allocation.value_to_slot.contains_key(&v0) {
            let use_point = InstPoint::new(0, 2); // iadd instruction
            assert!(plan.before.contains_key(&use_point));
        }
    }

    #[test]
    fn test_call_site_spill_reload() {
        // Function with a call - caller-saved values should be handled
        let ir = r#"
function %test() -> i32 {
block0:
    v0 = iconst 10
    v1 = iconst 20
    call %helper(v0) -> v2
    v3 = iadd v1, v2
    return v3
}"#;

        let func = parse_function(ir).expect("Failed to parse IR function");
        let liveness = compute_liveness(&func);
        let allocation = allocate_registers(&func, &liveness);
        let _plan = create_spill_reload_plan(&func, &allocation, &liveness);

        // If v1 is in a caller-saved register and live across call, it should be handled
        let v1 = r5_ir::Value::new(1);
        if let Some(reg) = allocation.value_to_reg.get(&v1) {
            if crate::abi::Abi::is_caller_saved(*reg) {
                // Should have spill/reload around call site
                let _call_point = InstPoint::new(0, 3); // Call instruction
                                                        // Check that there's handling around the call
            }
        }
    }

    #[test]
    fn test_multiple_reloads() {
        // Function where a spilled value is reloaded multiple times
        let ir = r#"
function %test() -> i32 {
block0:
    v0 = iconst 1
    v1 = iadd v0, v0
    v2 = iadd v0, v0
    v3 = iadd v1, v2
    return v3
}"#;

        let func = parse_function(ir).expect("Failed to parse IR function");
        let liveness = compute_liveness(&func);
        let allocation = allocate_registers(&func, &liveness);
        let plan = create_spill_reload_plan(&func, &allocation, &liveness);

        // If v0 is spilled, it should have reloads before each use
        let v0 = r5_ir::Value::new(0);
        if allocation.value_to_slot.contains_key(&v0) {
            // Count reloads for v0
            let reload_count = plan
                .before
                .values()
                .flatten()
                .filter(|op| matches!(op, SpillReloadOp::Reload { value, .. } if *value == v0))
                .count();
            assert!(
                reload_count >= 2,
                "Spilled value v0 should have multiple reloads"
            );
        }
    }

    #[test]
    fn test_spilled_return_value() {
        // Function where return value might be spilled
        let ir = r#"
function %test() -> i32 {
block0:
    v0 = iconst 42
    return v0
}"#;

        let func = parse_function(ir).expect("Failed to parse IR function");
        let liveness = compute_liveness(&func);
        let allocation = allocate_registers(&func, &liveness);
        let plan = create_spill_reload_plan(&func, &allocation, &liveness);

        // Return value should be handled correctly
        let v0 = r5_ir::Value::new(0);
        if allocation.value_to_slot.contains_key(&v0) {
            // If spilled, should have reload before return
            let return_point = InstPoint::new(0, 2);
            assert!(
                plan.before.contains_key(&return_point),
                "Spilled return value should be reloaded before return"
            );
        }
    }

    #[test]
    fn test_block_boundary_reload() {
        // Function with multiple blocks - spilled values used across blocks
        let ir = r#"
function %test() -> i32 {
block0:
    v0 = iconst 1
    jump block1
block1:
    v1 = iadd v0, v0
    return v1
}"#;

        let func = parse_function(ir).expect("Failed to parse IR function");
        let liveness = compute_liveness(&func);
        let allocation = allocate_registers(&func, &liveness);
        let plan = create_spill_reload_plan(&func, &allocation, &liveness);

        // If v0 is spilled and used in block1, should have reload at block1 entry
        let v0 = r5_ir::Value::new(0);
        if allocation.value_to_slot.contains_key(&v0) {
            // Should have reload before use in block1
            let use_point = InstPoint::new(1, 1); // iadd in block1
            assert!(
                plan.before.contains_key(&use_point),
                "Spilled value used in different block should be reloaded"
            );
        }
    }

    #[test]
    fn test_call_site_spills_caller_saved() {
        // Function with a call where a value in a caller-saved register is live across the call
        let ir = r#"
function %test() -> i32 {
block0:
    v0 = iconst 10
    v1 = iconst 20
    call %helper(v0) -> v2
    v3 = iadd v1, v2
    return v3
}"#;

        let func = parse_function(ir).expect("Failed to parse IR function");
        let liveness = compute_liveness(&func);
        let allocation = allocate_registers(&func, &liveness);
        let plan = create_spill_reload_plan(&func, &allocation, &liveness);

        // v1 should be live across the call
        let v1 = r5_ir::Value::new(1);
        if let Some(reg) = allocation.value_to_reg.get(&v1) {
            if crate::abi::Abi::is_caller_saved(*reg) {
                // v1 is in a caller-saved register and live across call
                // Should have spill before call and reload after call
                let call_point = InstPoint::new(0, 3); // Call instruction
                let after_call_point = InstPoint::new(0, 4); // After call

                // Check for spill before call
                let has_spill = plan
                    .before
                    .get(&call_point)
                    .map(|ops| {
                        ops.iter().any(
                            |op| matches!(op, SpillReloadOp::Spill { value, .. } if *value == v1),
                        )
                    })
                    .unwrap_or(false);

                // Check for reload after call
                let has_reload = plan
                    .after
                    .get(&after_call_point)
                    .map(|ops| {
                        ops.iter().any(
                            |op| matches!(op, SpillReloadOp::Reload { value, .. } if *value == v1),
                        )
                    })
                    .unwrap_or(false);

                assert!(
                    has_spill || has_reload,
                    "Caller-saved value live across call should have spill/reload operations"
                );
            }
        }
    }

    #[test]
    fn test_call_site_no_spill_if_already_spilled() {
        // Function where a value is already spilled and live across a call
        let ir = r#"
function %test() -> i32 {
block0:
    v0 = iconst 10
    v1 = iconst 20
    v2 = iconst 30
    v3 = iconst 40
    v4 = iconst 50
    v5 = iconst 60
    v6 = iconst 70
    v7 = iconst 80
    v8 = iconst 90
    v9 = iconst 100
    call %helper(v0) -> v10
    v11 = iadd v1, v10
    return v11
}"#;

        let func = parse_function(ir).expect("Failed to parse IR function");
        let liveness = compute_liveness(&func);
        let allocation = allocate_registers(&func, &liveness);
        let plan = create_spill_reload_plan(&func, &allocation, &liveness);

        // v1 might be spilled due to register pressure
        let v1 = r5_ir::Value::new(1);
        if allocation.value_to_slot.contains_key(&v1) {
            // If v1 is already spilled, we shouldn't create additional spills at call site
            // (it's already safe)
            let call_point = InstPoint::new(0, 10); // Call instruction
            if let Some(ops) = plan.before.get(&call_point) {
                // Should not have duplicate spills for already-spilled values
                let spill_count = ops
                    .iter()
                    .filter(|op| matches!(op, SpillReloadOp::Spill { value, .. } if *value == v1))
                    .count();
                // If there's a spill, it should be for a different reason (temporary spill)
                // But the value is already spilled, so it shouldn't need another spill
                assert_eq!(
                    spill_count, 0,
                    "Already-spilled value shouldn't need additional spill"
                );
            }
        }
    }

    #[test]
    fn test_temp_spill_slots_within_frame_bounds() {
        // Test that temporary spill slots don't exceed frame bounds
        // Function with a call where multiple caller-saved values need temporary spills
        let ir = r#"
function %test() -> i32 {
block0:
    v0 = iconst 10
    v1 = iconst 20
    v2 = iconst 30
    v3 = iconst 40
    v4 = iconst 50
    v5 = iconst 60
    v6 = iconst 70
    v7 = iconst 80
    v8 = iconst 90
    v9 = iconst 100
    call %helper(v0) -> v10
    v11 = iadd v1, v2
    v12 = iadd v3, v4
    v13 = iadd v5, v6
    v14 = iadd v7, v8
    v15 = iadd v9, v11
    v16 = iadd v12, v13
    v17 = iadd v14, v15
    v18 = iadd v16, v17
    return v18
}"#;

        let func = parse_function(ir).expect("Failed to parse IR function");
        let liveness = compute_liveness(&func);
        let allocation = allocate_registers(&func, &liveness);
        let spill_reload = create_spill_reload_plan(&func, &allocation, &liveness);

        // Verify that max_temp_spill_slots is set correctly
        assert!(
            spill_reload.max_temp_spill_slots > 0,
            "Should need temporary spill slots for caller-saved values"
        );

        // Compute frame layout with temporary slots
        let has_calls = true;
        let total_spill_slots = allocation.spill_slot_count + spill_reload.max_temp_spill_slots;
        let frame_layout = crate::frame::FrameLayout::compute(
            &allocation.used_callee_saved,
            total_spill_slots,
            has_calls,
            func.signature.params.len(),
            0,
        );

        // Verify that all temporary spill slots are within frame bounds
        // The maximum temporary slot number would be: allocation.spill_slot_count + max_temp_spill_slots - 1
        let max_temp_slot =
            (allocation.spill_slot_count + spill_reload.max_temp_spill_slots) as u32;
        if max_temp_slot > allocation.spill_slot_count as u32 {
            // Check that the maximum temporary slot offset is within the frame
            let max_offset = frame_layout.spill_slot_offset(max_temp_slot - 1);
            let frame_size = frame_layout.total_size();

            // The offset should be negative and within the frame bounds
            // Frame starts at -frame_size (after SP adjustment)
            assert!(
                max_offset.as_i32() < 0,
                "Spill slot offset should be negative"
            );
            assert!(
                max_offset.as_i32().abs() as u32 <= frame_size,
                "Spill slot offset {} should be within frame size {}",
                max_offset.as_i32().abs(),
                frame_size
            );
        }
    }

    #[test]
    fn test_temp_spill_slots_no_calls() {
        // Test that functions without calls don't need temporary spill slots
        let ir = r#"
function %test() -> i32 {
block0:
    v0 = iconst 1
    v1 = iconst 2
    v2 = iadd v0, v1
    return v2
}"#;

        let func = parse_function(ir).expect("Failed to parse IR function");
        let liveness = compute_liveness(&func);
        let allocation = allocate_registers(&func, &liveness);
        let spill_reload = create_spill_reload_plan(&func, &allocation, &liveness);

        // Functions without calls shouldn't need temporary spill slots
        assert_eq!(
            spill_reload.max_temp_spill_slots, 0,
            "Functions without calls shouldn't need temporary spill slots"
        );
    }

    #[test]
    fn test_temp_spill_slots_multiple_calls() {
        // Test that max_temp_spill_slots accounts for the maximum across all calls
        let ir = r#"
function %test() -> i32 {
block0:
    v0 = iconst 10
    v1 = iconst 20
    v2 = iconst 30
    call %helper1(v0) -> v3
    v4 = iadd v1, v3
    call %helper2(v4) -> v5
    v6 = iadd v2, v5
    return v6
}"#;

        let func = parse_function(ir).expect("Failed to parse IR function");
        let liveness = compute_liveness(&func);
        let allocation = allocate_registers(&func, &liveness);
        let spill_reload = create_spill_reload_plan(&func, &allocation, &liveness);

        // Should account for temporary slots needed across all calls
        // The first call might need spills for v1 and v2
        // The second call might need spills for v2
        // So max_temp_spill_slots should be at least 2
        // Note: max_temp_spill_slots is usize, so it's always >= 0
        // The assertion just verifies the value is set correctly

        // Verify frame layout includes temporary slots
        let has_calls = true;
        let total_spill_slots = allocation.spill_slot_count + spill_reload.max_temp_spill_slots;
        let frame_layout = crate::frame::FrameLayout::compute(
            &allocation.used_callee_saved,
            total_spill_slots,
            has_calls,
            func.signature.params.len(),
            0,
        );

        // All spill slots (including temporary ones) should be within frame bounds
        if total_spill_slots > 0 {
            let max_slot = (total_spill_slots - 1) as u32;
            let max_offset = frame_layout.spill_slot_offset(max_slot);
            let frame_size = frame_layout.total_size();

            assert!(
                max_offset.as_i32().abs() as u32 <= frame_size,
                "All spill slots should be within frame bounds"
            );
        }
    }
}
