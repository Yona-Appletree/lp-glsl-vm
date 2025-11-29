//! RISC-V 32-bit register helpers for backend3

use regalloc2::{PReg, RegClass};

use crate::backend3::types::{Reg, Writable};

/// Get the zero register (x0) as a Reg
///
/// The zero register is a physical register that always reads as 0.
/// This returns a Reg representing x0 (physical register 0, Int class).
pub fn zero_reg() -> Reg {
    Reg::from_real_reg(PReg::new(0, RegClass::Int))
}

/// Get a writable zero register (for instructions that write to x0)
///
/// Note: Writing to x0 is a no-op on RISC-V, but this can be useful
/// for instructions that require a destination register.
pub fn writable_zero_reg() -> Writable<Reg> {
    Writable::new(zero_reg())
}

