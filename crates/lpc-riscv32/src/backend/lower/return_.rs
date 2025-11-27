//! Lower return instructions.

extern crate alloc;

use alloc::vec::Vec;

use lpc_lpir::Value;

use super::Lowerer;
use crate::{Gpr, Inst};

/// Lower RETURN: return values
pub fn lower_return(lowerer: &mut Lowerer, values: &[Value]) {
    // Step 1: Move return values to a0-a1
    // Collect source registers first to avoid borrow conflicts
    let ret_regs: Vec<(usize, Gpr)> = values
        .iter()
        .enumerate()
        .map(|(i, ret_value)| {
            if i >= 2 {
                panic!("Multi-return not yet implemented (return index {})", i);
            }
            let src_reg = lowerer
                .allocation
                .value_to_reg
                .get(ret_value)
                .copied()
                .expect("Return value not allocated to register");
            (i, src_reg)
        })
        .collect();

    for (i, src_reg) in ret_regs {
        let target_reg = if i == 0 { Gpr::A0 } else { Gpr::A1 };

        if src_reg != target_reg {
            lowerer
                .inst_buffer_mut()
                .push_add(target_reg, src_reg, Gpr::Zero);
        }
    }

    // Step 2: Generate full epilogue before returning
    // Following Cranelift's approach: generate epilogue at each return site
    // This includes:
    // 1. Restore clobbered callee-saved registers
    // 2. Restore RA and FP, deallocate frame
    // 3. Return instruction
    // Clone frame_layout to avoid borrow conflicts with inst_buffer_mut()
    let frame_layout = lowerer.frame_layout.clone();
    crate::backend::abi::gen_clobber_restore(lowerer.inst_buffer_mut(), &frame_layout);
    crate::backend::abi::gen_epilogue_frame_restore(lowerer.inst_buffer_mut(), &frame_layout);

    // Step 3: Emit return instruction
    // JALR x0, x1, 0 (return to caller)
    lowerer.inst_buffer_mut().emit(Inst::Jalr {
        rd: Gpr::Zero,
        rs1: Gpr::Ra,
        imm: 0,
    });
}
