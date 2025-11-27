//! ABI (Application Binary Interface) implementation for RISC-V 32-bit.
//!
//! This module handles argument and return value passing according to the
//! RISC-V 32-bit calling convention.

extern crate alloc;

use crate::{backend::frame::FrameLayout, inst_buffer::InstBuffer, Gpr};

/// ABI information for a function.
///
/// This tracks how arguments and return values are passed according to
/// the RISC-V 32-bit ABI.
#[derive(Clone, Debug)]
pub struct Abi {
    /// Number of arguments passed in registers (max 8 for RV32).
    pub num_reg_args: usize,

    /// Number of arguments passed on the stack.
    pub num_stack_args: usize,

    /// Number of return values passed in registers (max 2 for RV32).
    pub num_reg_rets: usize,

    /// Number of return values passed on the stack (multi-return).
    pub num_stack_rets: usize,

    /// Size of stack arguments in bytes.
    pub stack_args_size: u32,

    /// Size of stack return values in bytes.
    pub stack_rets_size: u32,

    /// Whether multi-return is enabled (requires return area).
    pub uses_return_area: bool,
}

impl Abi {
    /// Compute ABI information for a function signature.
    ///
    /// # Parameters
    ///
    /// - `num_params`: Number of function parameters
    /// - `num_returns`: Number of return values
    /// - `enable_multi_ret`: Whether multi-return is enabled
    ///
    /// # Returns
    ///
    /// ABI information describing how arguments and returns are passed.
    pub fn compute_abi_info(num_params: usize, num_returns: usize, enable_multi_ret: bool) -> Self {
        // RISC-V 32-bit: 8 argument registers (a0-a7, x10-x17)
        const MAX_REG_ARGS: usize = 8;
        // RISC-V 32-bit: 2 return registers (a0-a1, x10-x11)
        const MAX_REG_RETS: usize = 2;

        let num_reg_args = num_params.min(MAX_REG_ARGS);
        let num_stack_args = num_params.saturating_sub(MAX_REG_ARGS);

        // Each argument is 4 bytes (i32)
        let stack_args_size = (num_stack_args as u32) * 4;
        // Align stack args to 16 bytes
        let stack_args_size = (stack_args_size + 15) & !15;

        // Return values
        let num_reg_rets = if enable_multi_ret {
            num_returns.min(MAX_REG_RETS)
        } else {
            // Without multi-return, only 2 returns allowed
            if num_returns > MAX_REG_RETS {
                panic!(
                    "Too many return values: {} (max {})",
                    num_returns, MAX_REG_RETS
                );
            }
            num_returns
        };

        let num_stack_rets = if enable_multi_ret {
            num_returns.saturating_sub(MAX_REG_RETS)
        } else {
            0
        };

        // Each return value is 4 bytes (i32)
        let stack_rets_size = (num_stack_rets as u32) * 4;
        // Align stack returns to 16 bytes
        let stack_rets_size = (stack_rets_size + 15) & !15;

        let uses_return_area = num_stack_rets > 0;

        Self {
            num_reg_args,
            num_stack_args,
            num_reg_rets,
            num_stack_rets,
            stack_args_size,
            stack_rets_size,
            uses_return_area,
        }
    }
}

/// Generate instructions to adjust the stack pointer.
///
/// For small adjustments (within ±2047), uses a single `addi` instruction.
/// For larger adjustments, uses `lui` + `addi` to load the constant, then `add`.
fn gen_sp_reg_adjust(buf: &mut InstBuffer, amount: i32) {
    if amount == 0 {
        return;
    }

    // Check if amount fits in 12-bit signed immediate
    if amount >= -2048 && amount <= 2047 {
        // Single addi instruction
        buf.push_addi(Gpr::Sp, Gpr::Sp, amount);
    } else {
        // Need to load constant first
        // For now, we'll use a simple approach: load upper bits with lui,
        // then add lower bits with addi
        let upper = (amount >> 12) & 0xfffff;
        let lower = amount & 0xfff;

        // Load upper 20 bits
        buf.push_lui(Gpr::T0, upper as u32);

        // Add lower 12 bits if non-zero
        if lower != 0 {
            buf.push_addi(Gpr::T0, Gpr::T0, lower as i32);
        }

        // Add to SP
        buf.push_add(Gpr::Sp, Gpr::Sp, Gpr::T0);
    }
}

/// Generate prologue frame setup instructions.
///
/// This generates the sequence:
/// 1. Allocate setup area: `addi sp, sp, -8`
/// 2. Save return address: `sw ra, 4(sp)`
/// 3. Save old FP: `sw fp, 0(sp)`
/// 4. Set new FP: `add fp, sp, zero` (or `mv fp, sp` if we had a move instruction)
pub fn gen_prologue_frame_setup(buf: &mut InstBuffer, frame_layout: &FrameLayout) {
    if frame_layout.setup_area_size > 0 {
        // Allocate setup area (8 bytes for RV32: FP + RA)
        gen_sp_reg_adjust(buf, -8);

        // Save return address at SP+4
        buf.push_sw(Gpr::Sp, Gpr::Ra, 4);

        // Save old FP at SP+0
        buf.push_sw(Gpr::Sp, Gpr::S0, 0); // FP is s0 (x8)

        // Set new FP: fp = sp (using add with zero)
        buf.push_add(Gpr::S0, Gpr::Sp, Gpr::Zero);
    }
}

/// Generate epilogue frame restore instructions.
///
/// This generates the sequence:
/// 1. Restore RA: `lw ra, 4(sp)`
/// 2. Restore FP: `lw fp, 0(sp)`
/// 3. Deallocate setup: `addi sp, sp, 8`
pub fn gen_epilogue_frame_restore(buf: &mut InstBuffer, frame_layout: &FrameLayout) {
    if frame_layout.setup_area_size > 0 {
        // Restore return address from SP+4
        buf.push_lw(Gpr::Ra, Gpr::Sp, 4);

        // Restore old FP from SP+0
        buf.push_lw(Gpr::S0, Gpr::Sp, 0); // FP is s0 (x8)

        // Deallocate setup area
        gen_sp_reg_adjust(buf, 8);
    }
}

/// Generate instructions to save clobbered callee-saved registers.
///
/// Registers are saved at offsets from SP, stored from top downward.
pub fn gen_clobber_save(buf: &mut InstBuffer, frame_layout: &FrameLayout) {
    let stack_size = frame_layout.clobber_size
        + frame_layout.fixed_frame_storage_size
        + frame_layout.outgoing_args_size;

    if stack_size > 0 {
        // Adjust SP downward for clobbers, fixed frame, and outgoing args
        gen_sp_reg_adjust(buf, -(stack_size as i32));

        // Save each clobbered register
        // Registers are stored from top downward (highest offset first)
        let mut cur_offset = 0;
        for reg in &frame_layout.clobbered_callee_saves {
            // Each register is 4 bytes, aligned to 4 bytes
            cur_offset = (cur_offset + 3) & !3; // Align to 4 bytes

            // Calculate offset from SP (stored from top downward)
            let offset = stack_size - cur_offset - 4;

            buf.push_sw(Gpr::Sp, *reg, offset as i32);

            cur_offset += 4;
        }
    }
}

/// Generate instructions to restore clobbered callee-saved registers.
///
/// Registers are restored from the same offsets where they were saved.
pub fn gen_clobber_restore(buf: &mut InstBuffer, frame_layout: &FrameLayout) {
    let stack_size = frame_layout.clobber_size
        + frame_layout.fixed_frame_storage_size
        + frame_layout.outgoing_args_size;

    if stack_size > 0 {
        // Restore each clobbered register
        let mut cur_offset = 0;
        for reg in &frame_layout.clobbered_callee_saves {
            // Each register is 4 bytes, aligned to 4 bytes
            cur_offset = (cur_offset + 3) & !3; // Align to 4 bytes

            // Calculate offset from SP (same as save)
            let offset = stack_size - cur_offset - 4;

            buf.push_lw(*reg, Gpr::Sp, offset as i32);

            cur_offset += 4;
        }

        // Restore SP
        gen_sp_reg_adjust(buf, stack_size as i32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_abi_simple_args() {
        let abi = Abi::compute_abi_info(3, 1, false);
        assert_eq!(abi.num_reg_args, 3);
        assert_eq!(abi.num_stack_args, 0);
        assert_eq!(abi.num_reg_rets, 1);
        assert_eq!(abi.num_stack_rets, 0);
        assert!(!abi.uses_return_area);
    }

    #[test]
    fn test_abi_many_args() {
        let abi = Abi::compute_abi_info(10, 1, false);
        assert_eq!(abi.num_reg_args, 8);
        assert_eq!(abi.num_stack_args, 2);
        assert_eq!(abi.stack_args_size, 16); // 2 args * 4 bytes = 8, aligned to 16
    }

    #[test]
    fn test_abi_multi_return() {
        let abi = Abi::compute_abi_info(0, 5, true);
        assert_eq!(abi.num_reg_rets, 2);
        assert_eq!(abi.num_stack_rets, 3);
        assert_eq!(abi.stack_rets_size, 16); // 3 rets * 4 bytes = 12, aligned to 16
        assert!(abi.uses_return_area);
    }

    #[test]
    fn test_abi_max_reg_args() {
        let abi = Abi::compute_abi_info(8, 0, false);
        assert_eq!(abi.num_reg_args, 8);
        assert_eq!(abi.num_stack_args, 0);
    }

    #[test]
    fn test_abi_max_reg_rets() {
        let abi = Abi::compute_abi_info(0, 2, false);
        assert_eq!(abi.num_reg_rets, 2);
        assert_eq!(abi.num_stack_rets, 0);
    }

    #[test]
    fn test_prologue_frame_setup() {
        use crate::backend::frame::{compute_frame_layout, FunctionCalls};

        let layout = compute_frame_layout(&[], FunctionCalls::Regular, 0, 0, 0, 0, 0, false);
        let mut buf = InstBuffer::new();
        gen_prologue_frame_setup(&mut buf, &layout);

        buf.assert_asm(
            "
            addi sp, sp, -8
            sw ra, 4(sp)
            sw s0, 0(sp)
            add s0, sp, zero
        ",
        );
    }

    #[test]
    fn test_prologue_no_setup() {
        use crate::backend::frame::{compute_frame_layout, FunctionCalls};

        let layout = compute_frame_layout(&[], FunctionCalls::None, 0, 0, 0, 0, 0, false);
        let mut buf = InstBuffer::new();
        gen_prologue_frame_setup(&mut buf, &layout);

        // No setup area, so no instructions
        assert_eq!(buf.instruction_count(), 0);
    }

    #[test]
    fn test_epilogue_frame_restore() {
        use crate::backend::frame::{compute_frame_layout, FunctionCalls};

        let layout = compute_frame_layout(&[], FunctionCalls::Regular, 0, 0, 0, 0, 0, false);
        let mut buf = InstBuffer::new();
        gen_epilogue_frame_restore(&mut buf, &layout);

        buf.assert_asm(
            "
            lw ra, 4(sp)
            lw s0, 0(sp)
            addi sp, sp, 8
        ",
        );
    }

    #[test]
    fn test_clobber_save() {
        use crate::backend::frame::{compute_frame_layout, FunctionCalls};

        let layout = compute_frame_layout(
            &[Gpr::S1, Gpr::S2],
            FunctionCalls::Regular,
            0,
            0,
            0,
            0,
            0,
            false,
        );
        let mut buf = InstBuffer::new();
        gen_clobber_save(&mut buf, &layout);

        // Should adjust SP and save 2 registers
        // Stack size = 16 (clobber) + 0 + 0 = 16
        // Adjust SP: addi sp, sp, -16
        // Save s1 at offset 12, s2 at offset 8
        buf.assert_asm(
            "
            addi sp, sp, -16
            sw s1, 12(sp)
            sw s2, 8(sp)
        ",
        );
    }

    #[test]
    fn test_clobber_restore() {
        use crate::backend::frame::{compute_frame_layout, FunctionCalls};

        let layout = compute_frame_layout(
            &[Gpr::S1, Gpr::S2],
            FunctionCalls::Regular,
            0,
            0,
            0,
            0,
            0,
            false,
        );
        let mut buf = InstBuffer::new();
        gen_clobber_restore(&mut buf, &layout);

        // Should restore 2 registers and adjust SP
        buf.assert_asm(
            "
            lw s1, 12(sp)
            lw s2, 8(sp)
            addi sp, sp, 16
        ",
        );
    }

    #[test]
    fn test_sp_reg_adjust_small() {
        let mut buf = InstBuffer::new();
        gen_sp_reg_adjust(&mut buf, -8);
        buf.assert_asm("addi sp, sp, -8");

        let mut buf = InstBuffer::new();
        gen_sp_reg_adjust(&mut buf, 16);
        buf.assert_asm("addi sp, sp, 16");
    }

    #[test]
    fn test_sp_reg_adjust_zero() {
        let mut buf = InstBuffer::new();
        gen_sp_reg_adjust(&mut buf, 0);
        assert_eq!(buf.instruction_count(), 0);
    }
}
