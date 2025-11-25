//! Simple register allocator for RISC-V 32-bit.

use alloc::collections::BTreeMap;

use r5_ir::Value;
use riscv32_encoder::Gpr;

/// A simple register allocator that maps IR values to RISC-V registers.
///
/// This is a very basic allocator that assigns registers sequentially.
/// A more sophisticated allocator would track live ranges and reuse registers.
pub struct SimpleRegAllocator {
    /// Map from IR Value to assigned register
    value_to_reg: BTreeMap<Value, Gpr>,
    /// Next available register to assign
    next_reg: u8,
}

impl SimpleRegAllocator {
    /// Create a new register allocator.
    pub fn new() -> Self {
        Self {
            value_to_reg: BTreeMap::new(),
            next_reg: 10, // Start with a0 (argument registers)
        }
    }

    /// Allocate a register for a value.
    ///
    /// Returns the assigned register. If the value already has a register,
    /// returns that register.
    pub fn allocate(&mut self, value: Value) -> Gpr {
        if let Some(&reg) = self.value_to_reg.get(&value) {
            return reg;
        }

        // Simple allocation: assign next available register
        // Use a0-a7 (10-17) first, then t0-t6 (5-7, 28-31), then s0-s11 (8-9, 18-27)
        let reg = if self.next_reg < 18 {
            // Use a0-a7
            Gpr::new(self.next_reg)
        } else if self.next_reg < 21 {
            // Use t0-t2
            Gpr::new(self.next_reg - 13) // 18->5, 19->6, 20->7
        } else if self.next_reg < 33 {
            // Use t3-t6 (28-31)
            let t_reg = self.next_reg - 13;
            if t_reg < 32 {
                Gpr::new(t_reg)
            } else {
                // Out of registers - for now, panic
                // TODO: Implement spilling
                panic!("Out of registers! Need to implement spilling.");
            }
        } else {
            // Out of registers - for now, panic
            // TODO: Implement spilling
            panic!("Out of registers! Need to implement spilling.");
        };

        self.value_to_reg.insert(value, reg);
        self.next_reg += 1;
        reg
    }

    /// Get the register for a value, if allocated.
    pub fn get(&self, value: Value) -> Option<Gpr> {
        self.value_to_reg.get(&value).copied()
    }

    /// Clear all allocations.
    pub fn clear(&mut self) {
        self.value_to_reg.clear();
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

        let r1 = alloc.allocate(v1);
        assert_eq!(r1.num(), 10); // a0

        let r2 = alloc.allocate(v2);
        assert_eq!(r2.num(), 11); // a1

        // Re-allocating same value returns same register
        let r1_again = alloc.allocate(v1);
        assert_eq!(r1_again.num(), 10);
    }
}
