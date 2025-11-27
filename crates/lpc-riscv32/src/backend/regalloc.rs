//! Register allocation for RISC-V 32-bit.

extern crate alloc;

use alloc::{
    collections::{BTreeMap, BTreeSet},
    vec::Vec,
};

use lpc_lpir::{Function, Value};

use super::liveness::{compute_liveness, InstPoint, LiveRange, LivenessInfo};
use crate::Gpr;

/// Register allocation result
pub struct RegisterAllocation {
    /// Value -> Register mapping (for values in registers)
    pub value_to_reg: BTreeMap<Value, Gpr>,
    /// Value -> Spill slot mapping (for spilled values)
    pub value_to_slot: BTreeMap<Value, u32>,
    /// Register -> Value mapping (reverse lookup, for active intervals)
    /// Using Vec of pairs since Gpr doesn't implement Ord
    pub reg_to_value: Vec<(Gpr, Value)>,
    /// Which callee-saved registers are used
    pub used_callee_saved: Vec<Gpr>,
    /// Number of spill slots needed
    pub spill_slot_count: usize,
}

/// Active interval during linear scan
struct ActiveInterval {
    value: Value,
    reg: Gpr,
    live_range: LiveRange,
}

/// Linear scan register allocator
struct LinearScanAllocator {
    /// Available registers (caller-saved first, then callee-saved)
    available_regs: Vec<Gpr>,
    /// Currently active intervals
    active: Vec<ActiveInterval>,
    /// Spill slot counter
    next_spill_slot: u32,
    /// Callee-saved registers used
    used_callee_saved: Vec<Gpr>,
}

impl LinearScanAllocator {
    fn new() -> Self {
        // Register ordering: caller-saved first, then callee-saved
        // Caller-saved: a0-a7 (x10-x17), t0-t6 (x5-x7, x28-x31)
        // Callee-saved: s0-s11 (x8-x9, x18-x27)
        let mut available_regs = Vec::new();

        // Caller-saved registers (preferred)
        available_regs.push(Gpr::A0);
        available_regs.push(Gpr::A1);
        available_regs.push(Gpr::A2);
        available_regs.push(Gpr::A3);
        available_regs.push(Gpr::A4);
        available_regs.push(Gpr::A5);
        available_regs.push(Gpr::A6);
        available_regs.push(Gpr::A7);
        available_regs.push(Gpr::T0);
        available_regs.push(Gpr::T1);
        available_regs.push(Gpr::T2);
        available_regs.push(Gpr::T3);
        available_regs.push(Gpr::T4);
        available_regs.push(Gpr::T5);
        available_regs.push(Gpr::T6);

        // Callee-saved registers (used when needed)
        available_regs.push(Gpr::S1); // s0 is FP, skip it
        available_regs.push(Gpr::S2);
        available_regs.push(Gpr::S3);
        available_regs.push(Gpr::S4);
        available_regs.push(Gpr::S5);
        available_regs.push(Gpr::S6);
        available_regs.push(Gpr::S7);
        available_regs.push(Gpr::S8);
        available_regs.push(Gpr::S9);
        available_regs.push(Gpr::S10);
        available_regs.push(Gpr::S11);

        Self {
            available_regs,
            active: Vec::new(),
            next_spill_slot: 0,
            used_callee_saved: Vec::new(),
        }
    }

    /// Expire intervals that end before the current point
    fn expire_old_intervals(&mut self, current_point: InstPoint) {
        self.active.retain(|interval| {
            let expires = interval.live_range.last_use < current_point;
            if expires {
                // Free the register
                // Register is automatically freed when interval is removed
            }
            !expires
        });
    }

    /// Find an available register
    fn find_available_reg(&self) -> Option<Gpr> {
        // Find a register that's not currently in use
        let used_regs: Vec<Gpr> = self.active.iter().map(|i| i.reg).collect();

        for reg in &self.available_regs {
            if !used_regs.contains(reg) {
                return Some(*reg);
            }
        }

        None
    }

    /// Find the interval with the furthest next use (best candidate for spilling)
    fn find_spill_candidate(&self, current_point: InstPoint) -> Option<usize> {
        let mut best_idx = None;
        let mut furthest_use = current_point;

        for (idx, interval) in self.active.iter().enumerate() {
            if interval.live_range.last_use > furthest_use {
                furthest_use = interval.live_range.last_use;
                best_idx = Some(idx);
            }
        }

        best_idx
    }

    /// Allocate a register for a value
    fn allocate_for_value(
        &mut self,
        value: Value,
        live_range: LiveRange,
        current_point: InstPoint,
    ) -> Option<Gpr> {
        // Expire old intervals
        self.expire_old_intervals(current_point);

        // Try to find an available register
        if let Some(reg) = self.find_available_reg() {
            // Mark as callee-saved if it is
            if self.is_callee_saved(reg) && !self.used_callee_saved.contains(&reg) {
                self.used_callee_saved.push(reg);
            }

            // Add to active intervals
            self.active.push(ActiveInterval {
                value,
                reg,
                live_range: live_range.clone(),
            });

            return Some(reg);
        }

        // No register available - need to spill
        // Note: Spilling is handled by returning None and assigning a spill slot
        // The actual spill/reload instructions will be inserted by spill_reload.rs
        None
    }

    /// Check if a register is callee-saved
    fn is_callee_saved(&self, reg: Gpr) -> bool {
        matches!(
            reg,
            Gpr::S1
                | Gpr::S2
                | Gpr::S3
                | Gpr::S4
                | Gpr::S5
                | Gpr::S6
                | Gpr::S7
                | Gpr::S8
                | Gpr::S9
                | Gpr::S10
                | Gpr::S11
        )
    }
}

/// Allocate registers for a function
pub fn allocate_registers(func: &Function, liveness: &LivenessInfo) -> RegisterAllocation {
    let mut allocator = LinearScanAllocator::new();
    let mut value_to_reg = BTreeMap::new();
    let mut value_to_slot = BTreeMap::new();
    let mut reg_to_value = Vec::new();

    // Sort values by definition point
    let mut values_by_def: Vec<(InstPoint, Value, LiveRange)> = liveness
        .live_ranges
        .iter()
        .map(|(value, live_range)| (live_range.def, *value, live_range.clone()))
        .collect();
    values_by_def.sort_by_key(|(point, _, _)| *point);

    // Linear scan: allocate registers
    for (def_point, value, live_range) in values_by_def {
        // Try to allocate a register
        if let Some(reg) = allocator.allocate_for_value(value, live_range.clone(), def_point) {
            value_to_reg.insert(value, reg);
            reg_to_value.push((reg, value));
        } else {
            // Need to spill - assign a spill slot
            let slot = allocator.next_spill_slot;
            allocator.next_spill_slot += 1;
            value_to_slot.insert(value, slot);
        }
    }

    // Sort used_callee_saved by register number
    allocator.used_callee_saved.sort_by_key(|r| r.num());
    let used_callee_saved = allocator.used_callee_saved;

    RegisterAllocation {
        value_to_reg,
        value_to_slot,
        reg_to_value,
        used_callee_saved,
        spill_slot_count: allocator.next_spill_slot as usize,
    }
}

#[cfg(test)]
mod tests {
    use lpc_lpir::{Block, Inst, Signature, Type};

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
    fn test_allocate_simple() {
        let func = create_test_function();
        let liveness = compute_liveness(&func);
        let allocation = allocate_registers(&func, &liveness);

        // Should allocate registers for all values
        assert!(allocation.value_to_reg.len() > 0 || allocation.value_to_slot.len() > 0);
    }
}
