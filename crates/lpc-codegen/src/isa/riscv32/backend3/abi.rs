//! RISC-V 32-bit ABI machine specification for regalloc2
//!
//! This module provides ABI information for RISC-V 32-bit register allocation,
//! including MachineEnv creation for regalloc2.

use alloc::vec::Vec;
use regalloc2::{MachineEnv, PReg, RegClass};

/// RISC-V 32-bit ABI machine specification for regalloc2
///
/// This struct holds ABI information needed for register allocation.
#[derive(Debug, Clone)]
pub struct Riscv32ABI;

impl Riscv32ABI {
    /// RISC-V 32 callee-saved registers:
    /// s0 (x8/fp), s1 (x9), s2-s11 (x18-x27)
    pub const CALLEE_SAVED_GPRS: &'static [u8] = &[8, 9, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27];

    /// RISC-V 32 caller-saved registers:
    /// t0-t2 (x5-x7), a0-a7 (x10-x17), t3-t6 (x28-x31)
    pub const CALLER_SAVED_GPRS: &'static [u8] = &[5, 6, 7, 10, 11, 12, 13, 14, 15, 16, 17, 28, 29, 30, 31];

    /// Create a MachineEnv for RISC-V 32-bit register allocation
    ///
    /// This configures regalloc2 with the available physical registers for RISC-V 32.
    /// Preferred registers are caller-saved (temporaries), non-preferred are callee-saved.
    pub fn machine_env() -> MachineEnv {
        use alloc::vec;

        // Preferred registers: caller-saved temporaries (allocated first)
        // These are t0-t6, a0-a7 - good for temporary values
        let mut preferred_int = Vec::new();
        for &reg_num in Self::CALLER_SAVED_GPRS {
            preferred_int.push(PReg::new(reg_num as usize, RegClass::Int));
        }

        // Non-preferred registers: callee-saved (s0-s11)
        // These are saved across calls, so we prefer not to use them unless necessary
        let mut non_preferred_int = Vec::new();
        for &reg_num in Self::CALLEE_SAVED_GPRS {
            non_preferred_int.push(PReg::new(reg_num as usize, RegClass::Int));
        }

        // No floating-point or vector registers for now (RISC-V 32 integer-only)
        MachineEnv {
            preferred_regs_by_class: [
                preferred_int,      // Int
                vec![],              // Float (empty for now)
                vec![],              // Vector (empty for now)
            ],
            non_preferred_regs_by_class: [
                non_preferred_int,   // Int
                vec![],              // Float (empty for now)
                vec![],              // Vector (empty for now)
            ],
            scratch_by_class: [
                None,  // Int - let regalloc2 choose automatically
                None,  // Float
                None,  // Vector
            ],
            fixed_stack_slots: vec![], // No fixed stack slots for now
        }
    }
}

