//! ABI (Application Binary Interface) handling for RISC-V 32-bit.
//!
//! This module handles calling conventions, argument passing, and return values.

use alloc::vec::Vec;

use riscv32_encoder::Gpr;

use crate::regalloc::{is_callee_saved, is_caller_saved, RegisterAllocation};

/// ABI information for a function.
#[derive(Debug, Clone)]
pub struct AbiInfo {
    /// Parameter -> argument register mapping
    pub param_regs: alloc::collections::BTreeMap<usize, Gpr>,
    /// Return value -> return register mapping
    pub return_regs: alloc::collections::BTreeMap<usize, Gpr>,
    /// Which callee-saved registers are used
    pub used_callee_saved: Vec<Gpr>,
    /// Maximum outgoing arguments (for frame layout)
    pub max_outgoing_args: usize,
    /// Parameter -> stack offset mapping (for stack args, index >= 8)
    /// Offset is relative to SP before prologue (positive offset)
    pub param_stack_offsets: alloc::collections::BTreeMap<usize, i32>,
    /// Return value -> stack offset mapping (for stack returns, index >= 8)
    /// Offset is relative to SP before prologue (positive offset)
    pub return_stack_offsets: alloc::collections::BTreeMap<usize, i32>,
}

/// ABI helper functions.
pub struct Abi;

impl Abi {
    /// Get argument register for parameter index.
    ///
    /// Returns `Some(register)` for indices 0-7 (a0-a7), `None` for >7 (stack).
    pub fn arg_reg(index: usize) -> Option<Gpr> {
        match index {
            0 => Some(Gpr::A0),
            1 => Some(Gpr::A1),
            2 => Some(Gpr::A2),
            3 => Some(Gpr::A3),
            4 => Some(Gpr::A4),
            5 => Some(Gpr::A5),
            6 => Some(Gpr::A6),
            7 => Some(Gpr::A7),
            _ => None,
        }
    }

    /// Get return register for return value index.
    ///
    /// Returns `Some(register)` for indices 0-7 (a0-a7), `None` for >7 (stack).
    pub fn return_reg(index: usize) -> Option<Gpr> {
        Self::arg_reg(index) // Same as argument registers
    }

    /// Get all caller-saved registers.
    pub fn caller_saved_regs() -> Vec<Gpr> {
        let mut regs = Vec::new();
        // a0-a7 (10-17)
        for i in 10..=17 {
            regs.push(Gpr::new(i));
        }
        // t0-t2 (5-7)
        for i in 5..=7 {
            regs.push(Gpr::new(i));
        }
        // t3-t6 (28-31)
        for i in 28..=31 {
            regs.push(Gpr::new(i));
        }
        // ra (1)
        regs.push(Gpr::RA);
        regs
    }

    /// Get all callee-saved registers.
    pub fn callee_saved_regs() -> Vec<Gpr> {
        let mut regs = Vec::new();
        // s0-s1 (8-9)
        for i in 8..=9 {
            regs.push(Gpr::new(i));
        }
        // s2-s11 (18-27)
        for i in 18..=27 {
            regs.push(Gpr::new(i));
        }
        regs
    }

    /// Check if register is caller-saved.
    pub fn is_caller_saved(reg: Gpr) -> bool {
        is_caller_saved(reg)
    }

    /// Check if register is callee-saved.
    pub fn is_callee_saved(reg: Gpr) -> bool {
        is_callee_saved(reg)
    }

    /// Compute ABI info for a function.
    pub fn compute_abi_info(func: &r5_ir::Function, allocation: &RegisterAllocation) -> AbiInfo {
        // Map parameters to argument registers
        let mut param_regs = alloc::collections::BTreeMap::new();
        let mut param_stack_offsets = alloc::collections::BTreeMap::new();
        if let Some(entry_block) = func.blocks.first() {
            for (i, param) in entry_block.params.iter().enumerate() {
                if let Some(reg) = Self::arg_reg(i) {
                    // Check if this parameter is actually in a register (might be spilled)
                    if allocation.value_to_reg.contains_key(param) {
                        param_regs.insert(i, reg);
                    }
                } else {
                    // Parameter index >= 8, goes on stack
                    // Stack offset is (index - 8) * 4, relative to SP before prologue
                    let stack_index = i - 8;
                    let stack_offset = (stack_index * 4) as i32;
                    param_stack_offsets.insert(i, stack_offset);
                }
            }
        }

        // Map return values to return registers
        let mut return_regs = alloc::collections::BTreeMap::new();
        let mut return_stack_offsets = alloc::collections::BTreeMap::new();
        for (i, _) in func.signature.returns.iter().enumerate() {
            if let Some(reg) = Self::return_reg(i) {
                return_regs.insert(i, reg);
            } else {
                // Return value index >= 8, goes on stack
                // Stack offset is (index - 8) * 4, relative to SP before prologue
                let stack_index = i - 8;
                let stack_offset = (stack_index * 4) as i32;
                return_stack_offsets.insert(i, stack_offset);
            }
        }

        // Get used callee-saved registers from allocation
        let used_callee_saved = allocation.used_callee_saved.clone();

        // Compute max outgoing arguments (for now, assume max 8)
        // TODO: Analyze actual call sites
        let max_outgoing_args = 8;

        AbiInfo {
            param_regs,
            return_regs,
            used_callee_saved,
            max_outgoing_args,
            param_stack_offsets,
            return_stack_offsets,
        }
    }
}

#[cfg(test)]
mod tests {
    use r5_ir::parse_function;

    use super::*;

    #[test]
    fn test_arg_regs() {
        assert_eq!(Abi::arg_reg(0), Some(Gpr::A0));
        assert_eq!(Abi::arg_reg(1), Some(Gpr::A1));
        assert_eq!(Abi::arg_reg(7), Some(Gpr::A7));
        assert_eq!(Abi::arg_reg(8), None); // Stack
    }

    #[test]
    fn test_return_regs() {
        assert_eq!(Abi::return_reg(0), Some(Gpr::A0));
        assert_eq!(Abi::return_reg(1), Some(Gpr::A1));
        assert_eq!(Abi::return_reg(7), Some(Gpr::A7));
        assert_eq!(Abi::return_reg(8), None); // Stack
    }

    #[test]
    fn test_caller_saved() {
        assert!(Abi::is_caller_saved(Gpr::A0));
        assert!(Abi::is_caller_saved(Gpr::T0));
        assert!(!Abi::is_caller_saved(Gpr::S0));
    }

    #[test]
    fn test_callee_saved() {
        assert!(Abi::is_callee_saved(Gpr::S0));
        assert!(Abi::is_callee_saved(Gpr::S1));
        assert!(!Abi::is_callee_saved(Gpr::A0));
    }

    #[test]
    fn test_caller_saved_regs_list() {
        let regs = Abi::caller_saved_regs();
        assert!(regs.contains(&Gpr::A0));
        assert!(regs.contains(&Gpr::T0));
        assert!(regs.contains(&Gpr::RA));
    }

    #[test]
    fn test_callee_saved_regs_list() {
        let regs = Abi::callee_saved_regs();
        assert!(regs.contains(&Gpr::S0));
        assert!(regs.contains(&Gpr::S1));
        assert!(!regs.contains(&Gpr::A0));
    }

    #[test]
    fn test_compute_abi_info_simple() {
        // Simple function with parameters
        let ir = r#"
function %test(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = iadd v0, v1
    return v2
}"#;

        let func = parse_function(ir).expect("Failed to parse IR function");
        let liveness = crate::liveness::compute_liveness(&func);
        let allocation = crate::regalloc::allocate_registers(&func, &liveness);
        let abi_info = Abi::compute_abi_info(&func, &allocation);

        // Should have parameter mappings
        assert!(abi_info.param_regs.contains_key(&0));
        assert!(abi_info.param_regs.contains_key(&1));
    }

    #[test]
    fn test_compute_abi_info_many_args() {
        // Function with many arguments (some on stack)
        let ir = r#"
function %test(i32, i32, i32, i32, i32, i32, i32, i32, i32, i32) -> i32 {
block0(v0: i32, v1: i32, v2: i32, v3: i32, v4: i32, v5: i32, v6: i32, v7: i32, v8: i32, v9: i32):
    v10 = iadd v0, v9
    return v10
}"#;

        let func = parse_function(ir).expect("Failed to parse IR function");
        let liveness = crate::liveness::compute_liveness(&func);
        let allocation = crate::regalloc::allocate_registers(&func, &liveness);
        let abi_info = Abi::compute_abi_info(&func, &allocation);

        // First 8 parameters should be in registers
        for i in 0..8 {
            if let Some(param) = func.blocks[0].params.get(i) {
                if allocation.value_to_reg.contains_key(param) {
                    assert!(abi_info.param_regs.contains_key(&i));
                }
            }
        }
    }

    #[test]
    fn test_compute_abi_info_return_values() {
        // Function with return values
        let ir = r#"
function %test() -> i32, i32 {
block0:
    v0 = iconst 1
    v1 = iconst 2
    return v0 v1
}"#;

        let func = parse_function(ir).expect("Failed to parse IR function");
        let liveness = crate::liveness::compute_liveness(&func);
        let allocation = crate::regalloc::allocate_registers(&func, &liveness);
        let abi_info = Abi::compute_abi_info(&func, &allocation);

        // Should have return register mappings
        assert!(abi_info.return_regs.contains_key(&0));
        assert!(abi_info.return_regs.contains_key(&1));
        assert_eq!(abi_info.return_regs.get(&0), Some(&Gpr::A0));
        assert_eq!(abi_info.return_regs.get(&1), Some(&Gpr::A1));
    }

    #[test]
    fn test_arg_reg_all_indices() {
        // Test all argument register indices
        for i in 0..8 {
            let reg = Abi::arg_reg(i);
            assert!(reg.is_some(), "arg_reg({}) should return Some", i);
            let expected_reg = match i {
                0 => Gpr::A0,
                1 => Gpr::A1,
                2 => Gpr::A2,
                3 => Gpr::A3,
                4 => Gpr::A4,
                5 => Gpr::A5,
                6 => Gpr::A6,
                7 => Gpr::A7,
                _ => unreachable!(),
            };
            assert_eq!(reg, Some(expected_reg));
        }

        // Index 8+ should return None (stack)
        assert_eq!(Abi::arg_reg(8), None);
        assert_eq!(Abi::arg_reg(9), None);
        assert_eq!(Abi::arg_reg(100), None);
    }

    #[test]
    fn test_return_reg_all_indices() {
        // Test all return register indices
        for i in 0..8 {
            let reg = Abi::return_reg(i);
            assert!(reg.is_some(), "return_reg({}) should return Some", i);
            // Should be same as arg_reg
            assert_eq!(reg, Abi::arg_reg(i));
        }

        // Index 8+ should return None (stack)
        assert_eq!(Abi::return_reg(8), None);
        assert_eq!(Abi::return_reg(9), None);
    }

    #[test]
    fn test_abi_info_tracks_stack_params() {
        // Function with 10 parameters (8 in regs, 2 on stack)
        let ir = r#"
function %test(i32, i32, i32, i32, i32, i32, i32, i32, i32, i32) -> i32 {
block0(v0: i32, v1: i32, v2: i32, v3: i32, v4: i32, v5: i32, v6: i32, v7: i32, v8: i32, v9: i32):
    v10 = iadd v0, v9
    return v10
}"#;

        let func = parse_function(ir).expect("Failed to parse IR function");
        let liveness = crate::liveness::compute_liveness(&func);
        let allocation = crate::regalloc::allocate_registers(&func, &liveness);
        let abi_info = Abi::compute_abi_info(&func, &allocation);

        // Parameters 0-7 should be in registers (if allocated)
        // Parameters 8-9 should be on stack
        assert!(abi_info.param_stack_offsets.contains_key(&8));
        assert!(abi_info.param_stack_offsets.contains_key(&9));
        assert_eq!(abi_info.param_stack_offsets.get(&8), Some(&0)); // First stack arg at offset 0
        assert_eq!(abi_info.param_stack_offsets.get(&9), Some(&4)); // Second stack arg at offset 4
    }

    #[test]
    fn test_abi_info_tracks_stack_returns() {
        // Function with 10 return values (8 in regs, 2 on stack)
        let ir = r#"
function %test() -> i32, i32, i32, i32, i32, i32, i32, i32, i32, i32 {
block0:
    v0 = iconst 0
    v1 = iconst 1
    v2 = iconst 2
    v3 = iconst 3
    v4 = iconst 4
    v5 = iconst 5
    v6 = iconst 6
    v7 = iconst 7
    v8 = iconst 8
    v9 = iconst 9
    return v0 v1 v2 v3 v4 v5 v6 v7 v8 v9
}"#;

        let func = parse_function(ir).expect("Failed to parse IR function");
        let liveness = crate::liveness::compute_liveness(&func);
        let allocation = crate::regalloc::allocate_registers(&func, &liveness);
        let abi_info = Abi::compute_abi_info(&func, &allocation);

        // Return values 0-7 should be in registers
        assert!(abi_info.return_regs.contains_key(&0));
        assert!(abi_info.return_regs.contains_key(&7));

        // Return values 8-9 should be on stack
        assert!(abi_info.return_stack_offsets.contains_key(&8));
        assert!(abi_info.return_stack_offsets.contains_key(&9));
        assert_eq!(abi_info.return_stack_offsets.get(&8), Some(&0)); // First stack return at offset 0
        assert_eq!(abi_info.return_stack_offsets.get(&9), Some(&4)); // Second stack return at offset 4
    }

    #[test]
    fn test_abi_info_mixed_reg_and_stack() {
        // Function with 12 parameters (8 in regs, 4 on stack)
        let ir = r#"
function %test(i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32) -> i32 {
block0(v0: i32, v1: i32, v2: i32, v3: i32, v4: i32, v5: i32, v6: i32, v7: i32, v8: i32, v9: i32, v10: i32, v11: i32):
    v12 = iadd v0, v11
    return v12
}"#;

        let func = parse_function(ir).expect("Failed to parse IR function");
        let liveness = crate::liveness::compute_liveness(&func);
        let allocation = crate::regalloc::allocate_registers(&func, &liveness);
        let abi_info = Abi::compute_abi_info(&func, &allocation);

        // First 8 should be in registers (if allocated)
        // Parameters 8-11 should be on stack
        for i in 8..12 {
            assert!(abi_info.param_stack_offsets.contains_key(&i));
            let expected_offset = ((i - 8) * 4) as i32;
            assert_eq!(abi_info.param_stack_offsets.get(&i), Some(&expected_offset));
        }
    }
}
