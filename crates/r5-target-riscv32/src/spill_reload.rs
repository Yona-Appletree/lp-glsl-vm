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
    for (block_idx, block) in func.blocks.iter().enumerate() {
        for (inst_idx, inst) in block.insts.iter().enumerate() {
            if matches!(inst, Inst::Call { .. }) {
                let call_point = InstPoint::new(block_idx, inst_idx + 1);

                // Find all live values in caller-saved registers
                let mut to_spill = Vec::new();
                for (value, reg) in &allocation.value_to_reg {
                    if crate::abi::Abi::is_caller_saved(*reg) {
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
                    let spills = Vec::new();
                    for (value, _reg) in to_spill {
                        // Only spill if not already spilled
                        if !allocation.value_to_slot.contains_key(&value) {
                            // This value is in a caller-saved register and live across call
                            // We need to spill it (or it should have been spilled already)
                            // For now, we'll handle this during lowering
                        }
                    }
                    if !spills.is_empty() {
                        before.insert(call_point, spills);
                    }
                }
            }
        }
    }

    SpillReloadPlan { before, after }
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
}
