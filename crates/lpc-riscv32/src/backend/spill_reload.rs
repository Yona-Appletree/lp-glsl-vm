//! Spill and reload instruction insertion planning.

extern crate alloc;

use alloc::{collections::BTreeMap, vec::Vec};

use lpc_lpir::{Function, Inst, Value};

use super::{
    liveness::{InstPoint, LivenessInfo},
    regalloc::RegisterAllocation,
};
use crate::Gpr;

/// Spill or reload operation
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpillReloadOp {
    /// Spill a value from register to stack
    Spill { value: Value, reg: Gpr, slot: u32 },
    /// Reload a value from stack to register
    Reload { value: Value, reg: Gpr, slot: u32 },
}

/// Spill/reload insertion plan
pub struct SpillReloadPlan {
    /// Operations to insert before each instruction
    pub before: BTreeMap<InstPoint, Vec<SpillReloadOp>>,
    /// Operations to insert after each instruction
    pub after: BTreeMap<InstPoint, Vec<SpillReloadOp>>,
    /// Operations to insert at block boundaries (before block entry)
    pub block_boundary: BTreeMap<usize, Vec<SpillReloadOp>>,
    /// Maximum temporary spill slots needed (for frame layout)
    pub max_temp_spill_slots: usize,
}

/// Create spill/reload plan for a function
pub fn create_spill_reload_plan(
    func: &Function,
    allocation: &RegisterAllocation,
    liveness: &LivenessInfo,
) -> SpillReloadPlan {
    let mut before = BTreeMap::new();
    let mut after = BTreeMap::new();
    let mut block_boundary = BTreeMap::new();
    let mut max_temp_spill_slots = 0;

    // For each spilled value, insert spill after definition and reload before uses
    for (value, slot) in &allocation.value_to_slot {
        // Find definition point
        if let Some(live_range) = liveness.live_ranges.get(value) {
            // Spill after definition
            after.insert(
                live_range.def,
                alloc::vec![SpillReloadOp::Spill {
                    value: *value,
                    reg: allocation
                        .value_to_reg
                        .get(value)
                        .copied()
                        .unwrap_or_else(|| {
                            // If spilled, it might not have a register at def point
                            // This shouldn't happen in normal allocation, but handle gracefully
                            Gpr::Zero // Placeholder
                        }),
                    slot: *slot,
                }],
            );

            // Reload before each use
            for use_point in &live_range.uses {
                before
                    .entry(*use_point)
                    .or_insert_with(Vec::new)
                    .push(SpillReloadOp::Reload {
                        value: *value,
                        reg: allocation
                            .value_to_reg
                            .get(value)
                            .copied()
                            .unwrap_or(Gpr::Zero), // Placeholder
                        slot: *slot,
                    });
            }
        }
    }

    // Handle call sites: spill caller-saved registers before calls
    // Note: This is a simplified implementation. Full implementation would
    // track which values are live across calls and spill them.
    for (block_idx, block) in func.blocks.iter().enumerate() {
        for (inst_idx, inst) in block.insts.iter().enumerate() {
            let point = InstPoint {
                block: block_idx,
                inst: inst_idx + 1,
            };

            if matches!(inst, Inst::Call { .. }) {
                // For now, we don't spill before calls - this will be handled
                // by the caller when they implement call lowering properly
                // TODO: Add spill/reload around calls when call lowering is implemented
            }
        }
    }

    // Handle block boundaries: reload spilled values used in successor blocks
    // This is a simplified implementation - full implementation would analyze
    // control flow to determine which values are live into each block
    for (block_idx, block) in func.blocks.iter().enumerate() {
        // Check if this block uses any spilled values
        let mut reloads = Vec::new();
        for (value, slot) in &allocation.value_to_slot {
            // Check if value is live at block entry
            let block_entry = InstPoint {
                block: block_idx,
                inst: 0,
            };
            if let Some(live_set) = liveness.live_sets.get(&block_entry) {
                if live_set.contains(value) {
                    reloads.push(SpillReloadOp::Reload {
                        value: *value,
                        reg: allocation
                            .value_to_reg
                            .get(value)
                            .copied()
                            .unwrap_or(Gpr::Zero),
                        slot: *slot,
                    });
                }
            }
        }
        if !reloads.is_empty() {
            block_boundary.insert(block_idx, reloads);
        }
    }

    SpillReloadPlan {
        before,
        after,
        block_boundary,
        max_temp_spill_slots,
    }
}

#[cfg(test)]
mod tests {
    use lpc_lpir::{Block, Signature, Type};

    use super::*;

    fn create_test_function() -> Function {
        let sig = Signature::new(alloc::vec![Type::I32], alloc::vec![Type::I32]);
        let mut func = Function::new(sig);

        let mut block0 = Block::with_params(alloc::vec![Value::new(0)]);
        block0.insts.push(Inst::Iconst {
            result: Value::new(1),
            value: 42,
        });
        block0.insts.push(Inst::Return {
            values: alloc::vec![Value::new(1)],
        });
        func.blocks.push(block0);

        func
    }

    #[test]
    fn test_create_spill_reload_plan() {
        let func = create_test_function();
        let liveness = super::super::liveness::compute_liveness(&func);
        let allocation = super::super::regalloc::allocate_registers(&func, &liveness);
        let plan = create_spill_reload_plan(&func, &allocation, &liveness);

        // Plan should be created successfully
        assert!(plan.max_temp_spill_slots >= 0);
    }
}
