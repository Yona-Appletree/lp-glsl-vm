//! Register allocator for RISC-V 32-bit.
//!
//! This allocator tracks callee-saved register usage and supports spilling.

use alloc::{collections::BTreeMap, vec::Vec};

use r5_ir::Value;
use riscv32_encoder::Gpr;

/// A register allocator that maps IR values to RISC-V registers.
///
/// This allocator follows Cranelift's approach:
/// - Tracks which callee-saved registers are used
/// - Supports spilling to stack slots
/// - Allocates caller-saved registers first (a0-a7, t0-t6)
/// - Falls back to callee-saved registers (s0-s11) when needed
pub struct SimpleRegAllocator {
    /// Map from IR Value to assigned register
    value_to_reg: BTreeMap<Value, Gpr>,
    /// Map from IR Value to spill slot
    spill_slots: BTreeMap<Value, u32>,
    /// Next available spill slot
    next_spill_slot: u32,
    /// Next available register to assign (starting from caller-saved)
    next_reg: u8,
}

impl SimpleRegAllocator {
    /// Create a new register allocator.
    pub fn new() -> Self {
        Self {
            value_to_reg: BTreeMap::new(),
            spill_slots: BTreeMap::new(),
            next_spill_slot: 0,
            next_reg: 10, // Start with a0 (argument registers, caller-saved)
        }
    }

    /// Check if a register is caller-saved.
    fn is_caller_saved(reg: Gpr) -> bool {
        let num = reg.num();
        // a0-a7 (10-17), t0-t6 (5-7, 28-31), ra (1)
        matches!(num, 1 | 5..=7 | 10..=17 | 28..=31)
    }

    /// Check if a register is callee-saved.
    fn is_callee_saved(reg: Gpr) -> bool {
        let num = reg.num();
        // s0-s11 (8-9, 18-27)
        matches!(num, 8..=9 | 18..=27)
    }

    /// Allocate a register for a value.
    ///
    /// Returns the assigned register. If the value already has a register,
    /// returns that register. If the value is spilled or no registers available,
    /// returns None (caller must handle spilling/reloading).
    ///
    /// Register allocation order:
    /// 1. Caller-saved: a0-a7 (10-17), then t0-t6 (5-7, 28-31)
    /// 2. Callee-saved: s0-s11 (8-9, 18-27)
    pub fn allocate(&mut self, value: Value) -> Option<Gpr> {
        // If already allocated, return that register
        if let Some(&reg) = self.value_to_reg.get(&value) {
            return Some(reg);
        }

        // If spilled, return None (caller must handle reloading)
        if self.spill_slots.contains_key(&value) {
            return None;
        }

        // Find next available register
        // Order: a0-a7 (10-17), t0-t2 (5-7), t3-t6 (28-31), s0-s11 (8-9, 18-27)
        let reg = self.find_next_available_register();

        if let Some(reg) = reg {
            self.value_to_reg.insert(value, reg);
            self.next_reg = self.next_reg_from(reg);
        }

        reg
    }

    /// Find the next available register not already in use.
    fn find_next_available_register(&self) -> Option<Gpr> {
        let used_regs: alloc::collections::BTreeSet<u8> =
            self.value_to_reg.values().map(|r| r.num()).collect();

        // Allocation order: a0-a7 (10-17), t0-t2 (5-7), t3-t6 (28-31), s0-s11 (8-9, 18-27)
        let register_order = [
            // Caller-saved: a0-a7
            10, 11, 12, 13, 14, 15, 16, 17, // Caller-saved: t0-t2
            5, 6, 7, // Caller-saved: t3-t6
            28, 29, 30, 31, // Callee-saved: s0-s11
            8, 9, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27,
        ];

        for &reg_num in &register_order {
            if !used_regs.contains(&reg_num) {
                return Some(Gpr::new(reg_num));
            }
        }

        None
    }

    /// Get the next register number to try after the given register.
    fn next_reg_from(&self, reg: Gpr) -> u8 {
        let reg_num = reg.num();
        // Return next in sequence, wrapping to start if needed
        match reg_num {
            10..=16 => reg_num + 1,
            17 => 5, // After a7, go to t0
            5..=6 => reg_num + 1,
            7 => 28, // After t2, go to t3
            28..=30 => reg_num + 1,
            31 => 8, // After t6, go to s0
            8 => 9,
            9 => 18, // After s1, go to s2
            18..=26 => reg_num + 1,
            27 => 10, // Wrap to a0 (shouldn't happen, but for completeness)
            _ => 10,
        }
    }

    /// Allocate a spill slot for a value.
    ///
    /// Returns the spill slot number.
    pub fn spill(&mut self, value: Value) -> u32 {
        // If already spilled, return existing slot
        if let Some(&slot) = self.spill_slots.get(&value) {
            return slot;
        }

        // Remove from register allocation if present
        self.value_to_reg.remove(&value);

        // Allocate new spill slot
        let slot = self.next_spill_slot;
        self.spill_slots.insert(value, slot);
        self.next_spill_slot += 1;
        slot
    }

    /// Get the spill slot for a value, if spilled.
    pub fn get_spill_slot(&self, value: Value) -> Option<u32> {
        self.spill_slots.get(&value).copied()
    }

    /// Check if a value is spilled.
    pub fn is_spilled(&self, value: Value) -> bool {
        self.spill_slots.contains_key(&value)
    }

    /// Get the register for a value, if allocated.
    pub fn get(&self, value: Value) -> Option<Gpr> {
        self.value_to_reg.get(&value).copied()
    }

    /// Check if a value is already mapped to a register.
    pub fn is_mapped(&self, value: Value) -> bool {
        self.value_to_reg.contains_key(&value)
    }

    /// Map a value to a specific register (for function parameters).
    pub fn map_value_to_register(&mut self, value: Value, reg: Gpr) {
        // Remove from spill slots if present
        self.spill_slots.remove(&value);
        self.value_to_reg.insert(value, reg);
    }

    /// Remove a value from spill slots (for reloading).
    /// This allows the value to be allocated to a register.
    pub fn unspill(&mut self, value: Value) {
        self.spill_slots.remove(&value);
    }

    /// Get list of callee-saved registers currently in use.
    pub fn get_used_callee_saved(&self) -> Vec<Gpr> {
        self.value_to_reg
            .values()
            .filter(|&&reg| Self::is_callee_saved(reg))
            .copied()
            .collect()
    }

    /// Get the number of spill slots used.
    pub fn spill_slot_count(&self) -> usize {
        self.spill_slots.len()
    }

    /// Get all value-to-register mappings.
    pub fn get_all_mappings(&self) -> &BTreeMap<Value, Gpr> {
        &self.value_to_reg
    }

    /// Clear all allocations.
    pub fn clear(&mut self) {
        self.value_to_reg.clear();
        self.spill_slots.clear();
        self.next_spill_slot = 0;
        self.next_reg = 10;
    }
}

impl Default for SimpleRegAllocator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_allocator() {
        let mut alloc = SimpleRegAllocator::new();

        // Allocate some values
        let v1 = Value::new(1);
        let v2 = Value::new(2);

        let r1 = alloc.allocate(v1).unwrap();
        assert_eq!(r1.num(), 10); // a0

        let r2 = alloc.allocate(v2).unwrap();
        assert_eq!(r2.num(), 11); // a1

        // Re-allocating same value returns same register
        let r1_again = alloc.allocate(v1).unwrap();
        assert_eq!(r1_again.num(), 10);
    }

    #[test]
    fn test_is_callee_saved() {
        assert!(SimpleRegAllocator::is_callee_saved(Gpr::S0)); // x8
        assert!(SimpleRegAllocator::is_callee_saved(Gpr::S1)); // x9
        assert!(SimpleRegAllocator::is_callee_saved(Gpr::S2)); // x18
        assert!(SimpleRegAllocator::is_callee_saved(Gpr::S11)); // x27
        assert!(!SimpleRegAllocator::is_callee_saved(Gpr::A0)); // x10 (caller-saved)
        assert!(!SimpleRegAllocator::is_callee_saved(Gpr::T0)); // x5 (caller-saved)
    }

    #[test]
    fn test_is_caller_saved() {
        assert!(SimpleRegAllocator::is_caller_saved(Gpr::A0)); // x10
        assert!(SimpleRegAllocator::is_caller_saved(Gpr::T0)); // x5
        assert!(SimpleRegAllocator::is_caller_saved(Gpr::RA)); // x1
        assert!(!SimpleRegAllocator::is_caller_saved(Gpr::S0)); // x8 (callee-saved)
    }

    #[test]
    fn test_get_used_callee_saved() {
        let mut alloc = SimpleRegAllocator::new();

        // Allocate many values to force use of callee-saved registers
        let mut values = Vec::new();
        for i in 0..20 {
            values.push(Value::new(i));
        }

        // Allocate all values
        for v in &values {
            alloc.allocate(*v);
        }

        let used_callee_saved = alloc.get_used_callee_saved();
        // Should have used some callee-saved registers
        assert!(!used_callee_saved.is_empty());

        // All should be callee-saved
        for reg in &used_callee_saved {
            assert!(SimpleRegAllocator::is_callee_saved(*reg));
        }
    }

    #[test]
    fn test_spill() {
        let mut alloc = SimpleRegAllocator::new();
        let v = Value::new(1);

        // Allocate and then spill
        alloc.allocate(v);
        let slot = alloc.spill(v);

        assert_eq!(slot, 0);
        assert!(alloc.is_spilled(v));
        assert!(alloc.get(v).is_none());
        assert_eq!(alloc.spill_slot_count(), 1);
    }
}
