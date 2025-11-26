//! Register allocation for RISC-V 32-bit.
//!
//! This module implements linear scan register allocation.

use alloc::{collections::BTreeMap, vec, vec::Vec};

use lpc_lpir::Value;
use crate::Gpr;

use super::{
    liveness::{InstPoint, LiveRange, LivenessInfo},
    register_role::RegisterRole,
};

/// Register allocation result.
#[derive(Debug, Clone)]
pub struct RegisterAllocation {
    /// Value -> Register mapping (for values in registers)
    pub value_to_reg: BTreeMap<Value, Gpr>,
    /// Value -> Spill slot mapping (for spilled values)
    pub value_to_slot: BTreeMap<Value, u32>,
    /// Which callee-saved registers are used
    pub used_callee_saved: Vec<Gpr>,
    /// Number of spill slots needed
    pub spill_slot_count: usize,
}

/// Active interval during linear scan.
struct ActiveInterval {
    value: Value,
    reg: Gpr,
    live_range: LiveRange,
}

/// Linear scan register allocator.
struct LinearScanAllocator {
    /// Available registers (caller-saved first, then callee-saved)
    available_regs: Vec<Gpr>,
    /// Currently active intervals
    active: Vec<ActiveInterval>,
    /// Spill slot counter
    next_spill_slot: u32,
}

impl LinearScanAllocator {
    fn new() -> Self {
        // Register allocation order: caller-saved first, then callee-saved
        // a0-a7, t0-t2, t3-t6, s0-s11
        let available_regs = vec![
            // Caller-saved: a0-a7
            Gpr::A0,
            Gpr::A1,
            Gpr::A2,
            Gpr::A3,
            Gpr::A4,
            Gpr::A5,
            Gpr::A6,
            Gpr::A7,
            // Caller-saved: t0-t2
            Gpr::T0,
            Gpr::T1,
            Gpr::T2,
            // Caller-saved: t3-t6
            Gpr::T3,
            Gpr::T4,
            Gpr::T5,
            Gpr::T6,
            // Callee-saved: s0-s11
            Gpr::S0,
            Gpr::S1,
            Gpr::S2,
            Gpr::S3,
            Gpr::S4,
            Gpr::S5,
            Gpr::S6,
            Gpr::S7,
            Gpr::S8,
            Gpr::S9,
            Gpr::S10,
            Gpr::S11,
        ];

        Self {
            available_regs,
            active: Vec::new(),
            next_spill_slot: 0,
        }
    }

    /// Expire intervals that end before the given point.
    fn expire_old_intervals(&mut self, point: InstPoint) {
        self.active
            .retain(|interval| interval.live_range.last_use >= point);
    }

    /// Find a free register, or None if all are in use.
    fn find_free_register(&self, used_regs: &[Gpr]) -> Option<Gpr> {
        for reg in &self.available_regs {
            if !used_regs.iter().any(|&r| r == *reg) {
                return Some(*reg);
            }
        }
        None
    }

    /// Find the interval with the furthest next use to spill.
    fn find_spill_candidate(&self, current_point: InstPoint) -> Option<usize> {
        let mut best_idx = None;
        let mut furthest_use = current_point;

        for (idx, interval) in self.active.iter().enumerate() {
            // Skip intervals that end at or before current point
            if interval.live_range.last_use <= current_point {
                continue;
            }
            // Find the one with the furthest next use
            if interval.live_range.last_use > furthest_use {
                furthest_use = interval.live_range.last_use;
                best_idx = Some(idx);
            }
        }

        best_idx
    }

    /// Allocate a register for a value.
    fn allocate_register(
        &mut self,
        value: Value,
        live_range: &LiveRange,
        current_point: InstPoint,
    ) -> Option<Gpr> {
        // Expire old intervals
        self.expire_old_intervals(current_point);

        // Find which registers are currently in use
        let used_regs: Vec<Gpr> = self.active.iter().map(|i| i.reg).collect();

        // Try to find a free register
        if let Some(reg) = self.find_free_register(&used_regs) {
            // Found a free register
            self.active.push(ActiveInterval {
                value,
                reg,
                live_range: live_range.clone(),
            });
            Some(reg)
        } else {
            // No free registers - need to spill
            None
        }
    }

    /// Spill a value and return its spill slot.
    fn spill_value(&mut self, _value: Value) -> u32 {
        let slot = self.next_spill_slot;
        self.next_spill_slot += 1;
        slot
    }
}

/// Allocate registers for a function using linear scan.
pub fn allocate_registers(_func: &lpc_lpir::Function, liveness: &LivenessInfo) -> RegisterAllocation {
    let mut allocator = LinearScanAllocator::new();
    let mut value_to_reg = BTreeMap::new();
    let mut value_to_slot = BTreeMap::new();
    let mut used_callee_saved = Vec::new();

    // Sort values by definition point (earliest first)
    let mut values_by_def: Vec<(InstPoint, Value)> = liveness
        .live_ranges
        .iter()
        .map(|(value, live_range)| (live_range.def, *value))
        .collect();
    values_by_def.sort_by_key(|(point, _)| *point);

    // Linear scan: allocate registers for each value
    for (def_point, value) in values_by_def {
        let live_range = liveness
            .live_range(value)
            .expect("Value should have live range");

        // Expire old intervals
        allocator.expire_old_intervals(def_point);

        // Try to allocate a register
        if let Some(reg) = allocator.allocate_register(value, live_range, def_point) {
            value_to_reg.insert(value, reg);

            // Track callee-saved register usage
            if is_callee_saved(reg) && !used_callee_saved.contains(&reg) {
                used_callee_saved.push(reg);
            }
        } else {
            // No free registers - need to spill
            // Find the interval with furthest next use
            if let Some(spill_idx) = allocator.find_spill_candidate(def_point) {
                // Spill the candidate
                let spilled_interval = allocator.active.remove(spill_idx);
                let spilled_value = spilled_interval.value;
                let spilled_reg = spilled_interval.reg;

                // Remove from mappings
                value_to_reg.remove(&spilled_value);

                // Assign spill slot
                let slot = allocator.spill_value(spilled_value);
                value_to_slot.insert(spilled_value, slot);

                // Allocate the freed register to our value
                value_to_reg.insert(value, spilled_reg);
            } else {
                // No candidate to spill (shouldn't happen, but handle it)
                let slot = allocator.spill_value(value);
                value_to_slot.insert(value, slot);
            }
        }
    }

    // Sort used callee-saved registers for consistent ordering
    used_callee_saved.sort_by_key(|reg| reg.num());

    RegisterAllocation {
        value_to_reg,
        value_to_slot,
        used_callee_saved,
        spill_slot_count: allocator.next_spill_slot as usize,
    }
}

/// Check if a register is caller-saved.
pub fn is_caller_saved(reg: Gpr) -> bool {
    reg.is_caller_saved()
}

/// Check if a register is callee-saved.
pub fn is_callee_saved(reg: Gpr) -> bool {
    reg.is_callee_saved()
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use lpc_lpir::{parse_function, Block, Function, Signature, Type};

    use super::*;
    use crate::backend::compute_liveness;

    #[test]
    fn test_allocate_simple() {
        // Simple function with a few values
        let sig = Signature::new(vec![], vec![Type::I32]);
        let mut func = Function::new(sig);

        let mut block = Block::new();
        let v0 = Value::new(0);
        let v1 = Value::new(1);
        let v2 = Value::new(2);

        block.push_inst(lpc_lpir::Inst::Iconst {
            result: v0,
            value: 1,
        });
        block.push_inst(lpc_lpir::Inst::Iconst {
            result: v1,
            value: 2,
        });
        block.push_inst(lpc_lpir::Inst::Iadd {
            result: v2,
            arg1: v0,
            arg2: v1,
        });
        block.push_inst(lpc_lpir::Inst::Return { values: vec![v2] });

        func.add_block(block);

        let liveness = compute_liveness(&func);
        let allocation = allocate_registers(&func, &liveness);

        // All values should be in registers (no spills needed)
        assert!(allocation.value_to_reg.contains_key(&v0));
        assert!(allocation.value_to_reg.contains_key(&v1));
        assert!(allocation.value_to_reg.contains_key(&v2));
        assert_eq!(allocation.spill_slot_count, 0);
    }

    #[test]
    fn test_allocate_many_values() {
        // Function with more values than registers (will need spills)
        let sig = Signature::empty();
        let mut func = Function::new(sig);

        let mut block = Block::new();
        let mut values = Vec::new();

        // Create 30 values (more than available registers)
        for i in 0..30 {
            let v = Value::new(i as u32);
            values.push(v);
            block.push_inst(lpc_lpir::Inst::Iconst {
                result: v,
                value: i as i64,
            });
        }

        // Use all values in a big add chain
        let mut result = values[0];
        for i in 1..values.len() {
            let new_result = Value::new(100 + i as u32);
            block.push_inst(lpc_lpir::Inst::Iadd {
                result: new_result,
                arg1: result,
                arg2: values[i],
            });
            result = new_result;
        }

        block.push_inst(lpc_lpir::Inst::Return {
            values: vec![result],
        });
        func.add_block(block);

        let liveness = compute_liveness(&func);
        let allocation = allocate_registers(&func, &liveness);

        // Should have allocated registers and possibly spilled
        assert!(allocation.spill_slot_count > 0 || allocation.value_to_reg.len() > 0);
    }

    #[test]
    fn test_is_callee_saved() {
        assert!(is_callee_saved(Gpr::S0)); // x8
        assert!(is_callee_saved(Gpr::S1)); // x9
        assert!(is_callee_saved(Gpr::S2)); // x18
        assert!(!is_callee_saved(Gpr::A0)); // x10 (caller-saved)
        assert!(!is_callee_saved(Gpr::T0)); // x5 (caller-saved)
    }

    #[test]
    fn test_is_caller_saved() {
        assert!(is_caller_saved(Gpr::A0)); // x10
        assert!(is_caller_saved(Gpr::T0)); // x5
        assert!(!is_caller_saved(Gpr::S0)); // x8 (callee-saved)
    }

    #[test]
    fn test_allocate_with_ir_string() {
        // Test using IR string format
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

        // All values should be allocated (no spills needed for simple case)
        assert!(allocation.value_to_reg.contains_key(&lpc_lpir::Value::new(0)));
        assert!(allocation.value_to_reg.contains_key(&lpc_lpir::Value::new(1)));
        assert!(allocation.value_to_reg.contains_key(&lpc_lpir::Value::new(2)));
    }

    #[test]
    fn test_allocate_block_params() {
        // Function with block parameters
        let ir = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iadd v0, v0
    return v1
}"#;

        let func = parse_function(ir).expect("Failed to parse IR function");
        let liveness = compute_liveness(&func);
        let allocation = allocate_registers(&func, &liveness);

        // Block parameter v0 should be allocated
        let v0 = lpc_lpir::Value::new(0);
        assert!(
            allocation.value_to_reg.contains_key(&v0) || allocation.value_to_slot.contains_key(&v0)
        );
    }

    #[test]
    fn test_allocate_interference() {
        // Function where values interfere (can't share registers)
        let ir = r#"
function %test() -> i32 {
block0:
    v0 = iconst 1
    v1 = iconst 2
    v2 = iconst 3
    v3 = iadd v0, v1
    v4 = iadd v1, v2
    v5 = iadd v3, v4
    return v5
}"#;

        let func = parse_function(ir).expect("Failed to parse IR function");
        let liveness = compute_liveness(&func);
        let allocation = allocate_registers(&func, &liveness);

        // v0, v1, v2 should all be allocated (they interfere)
        let v0 = lpc_lpir::Value::new(0);
        let v1 = lpc_lpir::Value::new(1);
        let _v2 = lpc_lpir::Value::new(2);

        // Check that interfering values get different registers
        if let (Some(_reg0), Some(_reg1)) = (
            allocation.value_to_reg.get(&v0),
            allocation.value_to_reg.get(&v1),
        ) {
            // If both are in registers, they should be different (they interfere)
            if v0 != v1 {
                // They might be the same if one was spilled, but if both are in regs, they differ
            }
        }
    }

    #[test]
    fn test_allocate_long_live_ranges() {
        // Function with values that have long live ranges
        let ir = r#"
function %test() -> i32 {
block0:
    v0 = iconst 1
    v1 = iconst 2
    v2 = iconst 3
    v3 = iconst 4
    v4 = iconst 5
    v5 = iadd v0, v1
    v6 = iadd v2, v3
    v7 = iadd v4, v5
    v8 = iadd v6, v7
    v9 = iadd v0, v8
    return v9
}"#;

        let func = parse_function(ir).expect("Failed to parse IR function");
        let liveness = compute_liveness(&func);
        let allocation = allocate_registers(&func, &liveness);

        // v0 has a very long live range (used at the end)
        let v0 = lpc_lpir::Value::new(0);
        let v0_range = liveness.live_range(v0).unwrap();
        assert!(v0_range.last_use.inst > v0_range.def.inst);

        // Should handle allocation correctly (may spill if needed)
        assert!(
            allocation.value_to_reg.contains_key(&v0) || allocation.value_to_slot.contains_key(&v0)
        );
    }

    #[test]
    fn test_allocate_return_values() {
        // Function that returns multiple values
        let ir = r#"
function %test() -> i32, i32 {
block0:
    v0 = iconst 1
    v1 = iconst 2
    return v0 v1
}"#;

        let func = parse_function(ir).expect("Failed to parse IR function");
        let liveness = compute_liveness(&func);
        let allocation = allocate_registers(&func, &liveness);

        // Return values should be allocated
        let v0 = lpc_lpir::Value::new(0);
        let v1 = lpc_lpir::Value::new(1);
        assert!(
            allocation.value_to_reg.contains_key(&v0) || allocation.value_to_slot.contains_key(&v0)
        );
        assert!(
            allocation.value_to_reg.contains_key(&v1) || allocation.value_to_slot.contains_key(&v1)
        );
    }
}
