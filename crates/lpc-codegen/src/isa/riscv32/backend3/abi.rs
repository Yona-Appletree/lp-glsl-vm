//! RISC-V 32-bit ABI machine specification for regalloc2
//!
//! This module provides ABI information for RISC-V 32-bit register allocation,
//! including MachineEnv creation for regalloc2.

use alloc::vec::Vec;
use regalloc2::{MachineEnv, PReg, PRegSet, RegClass};

use crate::isa::riscv32::regs::Gpr;

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

    /// Get ABI argument registers for function parameters
    ///
    /// Returns a vector of physical registers used for passing function arguments
    /// according to the RISC-V 32 calling convention (System V ABI):
    /// - a0 (x10), a1 (x11), a2 (x12), a3 (x13), a4 (x14), a5 (x15), a6 (x16), a7 (x17)
    ///
    /// Up to 8 integer arguments can be passed in registers. Additional arguments
    /// would be passed on the stack (not handled here).
    pub fn arg_regs() -> Vec<PReg> {
        // RISC-V 32: a0-a7 (x10-x17)
        (10..=17)
            .map(|n| PReg::new(n, RegClass::Int))
            .collect()
    }

    /// Create a PRegSet from caller-saved registers
    ///
    /// This is used to represent clobbered registers for function calls.
    pub fn caller_saved_pregset() -> PRegSet {
        let mut set = PRegSet::default();
        for &reg_num in Self::CALLER_SAVED_GPRS {
            let preg = PReg::new(reg_num as usize, RegClass::Int);
            set.add(preg);
        }
        set
    }

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

/// Frame layout for a function
///
/// This structure describes the stack frame layout for a function, including
/// areas for setup (FP/RA), callee-saved registers, spill slots, and ABI requirements.
#[derive(Debug, Clone)]
pub struct FrameLayout {
    /// Setup area size (FP + RA): 8 bytes
    pub setup_area_size: u32,
    /// Clobber area size: space for callee-saved registers that are clobbered
    pub clobber_area_size: u32,
    /// Spill slots size: space for register spills from regalloc2
    pub spill_slots_size: u32,
    /// ABI size: space for ABI requirements (outgoing args, etc.)
    pub abi_size: u32,
    /// List of callee-saved registers that are clobbered
    pub clobbered_regs: Vec<Gpr>,
}

impl FrameLayout {
    /// Compute total frame size
    pub fn total_size(&self) -> u32 {
        self.setup_area_size
            + self.clobber_area_size
            + self.spill_slots_size
            + self.abi_size
    }

    /// Compute spill slot offset from SP (after prologue)
    ///
    /// Spill slots are at negative offsets from SP (stack grows down).
    /// The offset is computed as: -(setup_area + clobber_area + slot_index * slot_size)
    pub fn spill_slot_offset(&self, slot_index: usize) -> i32 {
        let base = self.setup_area_size + self.clobber_area_size;
        -(base as i32 + (slot_index as i32 * 4))
    }
}

/// Convert PReg to Gpr
///
/// This converts a regalloc2 PReg to a RISC-V 32 Gpr.
/// Panics if the register is not an integer register or has an invalid encoding.
pub fn preg_to_gpr(preg: PReg) -> Gpr {
    assert_eq!(
        preg.class(),
        RegClass::Int,
        "Only integer registers are supported"
    );
    let hw_enc = preg.hw_enc();
    assert!(hw_enc < 32, "Invalid register encoding: {}", hw_enc);
    Gpr::new(hw_enc as u8)
}

