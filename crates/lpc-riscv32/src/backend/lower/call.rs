//! Lower function call instructions.

extern crate alloc;

use alloc::vec::Vec;

use lpc_lpir::Value;

use super::Lowerer;
use crate::{Gpr, Inst};

/// Lower CALL: results = callee(args...)
pub fn lower_call(lowerer: &mut Lowerer, callee: &str, args: &[Value], results: &[Value]) {
    let num_args = args.len();
    let num_rets = results.len();

    // Compute ABI info for callee
    let callee_abi = crate::backend::abi::Abi::compute_abi_info(num_args, num_rets, true);

    // Step 1: Prepare register arguments (a0-a7)
    // Collect source registers first to avoid borrow conflicts
    let arg_regs: Vec<(usize, Gpr)> = args
        .iter()
        .enumerate()
        .map(|(i, arg_value)| {
            if i >= 8 {
                panic!("Stack args not yet implemented (arg index {})", i);
            }
            let src_reg = lowerer
                .allocation
                .value_to_reg
                .get(arg_value)
                .copied()
                .expect("Argument value not allocated to register");
            (i, src_reg)
        })
        .collect();

    for (i, src_reg) in arg_regs {
        // Get target register (a0-a7)
        let target_reg = match i {
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

        // Copy if different
        if src_reg != target_reg {
            lowerer
                .inst_buffer_mut()
                .push_add(target_reg, src_reg, Gpr::Zero);
        }
    }

    // Step 2: Emit call instruction
    // Record the call location for later relocation
    let call_inst_idx = lowerer.inst_buffer_mut().instruction_count();
    lowerer.inst_buffer_mut().emit(Inst::Jal {
        rd: Gpr::Ra,
        imm: 0, // Placeholder - will be fixed up with relocation
    });

    // Record relocation for function address
    lowerer.record_call_relocation(call_inst_idx, alloc::string::String::from(callee));

    // Step 3: Read return values from registers (a0-a1)
    // Collect target registers first to avoid borrow conflicts
    let result_regs: Vec<(usize, Gpr)> = results
        .iter()
        .enumerate()
        .map(|(i, result_value)| {
            if i >= 2 {
                panic!("Multi-return not yet implemented (return index {})", i);
            }
            let target_reg = lowerer
                .allocation
                .value_to_reg
                .get(result_value)
                .copied()
                .expect("Result value not allocated to register");
            (i, target_reg)
        })
        .collect();

    for (i, target_reg) in result_regs {
        let src_reg = if i == 0 { Gpr::A0 } else { Gpr::A1 };

        if target_reg != src_reg {
            lowerer
                .inst_buffer_mut()
                .push_add(target_reg, src_reg, Gpr::Zero);
        }
    }
}
