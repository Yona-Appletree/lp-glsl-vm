//! Instruction format validation.
//!
//! Validates that InstData structure matches opcode expectations.

use alloc::{format, string::String, vec::Vec};

use crate::{dfg::Opcode, Function, VerifierError};

/// Verify instruction format correctness
///
/// Checks that InstData structure matches the opcode's expected format:
/// - Correct number of arguments
/// - Correct number of results
/// - Required fields present (block_args, ty, imm)
/// - Forbidden fields absent
pub fn verify_format(function: &Function, errors: &mut Vec<VerifierError>) {
    for block in function.blocks() {
        for inst in function.block_insts(block) {
            if let Some(inst_data) = function.dfg.inst_data(inst) {
                verify_instruction_format(function, inst, inst_data, errors);
            }
        }
    }
}

/// Verify a single instruction's format
fn verify_instruction_format(
    function: &Function,
    inst: crate::entity::Inst,
    inst_data: &crate::dfg::InstData,
    errors: &mut Vec<VerifierError>,
) {
    match &inst_data.opcode {
        // Arithmetic ops: 2 args, 1 result, no block_args, no ty, no imm
        Opcode::Iadd | Opcode::Isub | Opcode::Imul | Opcode::Idiv | Opcode::Irem => {
            if inst_data.args.len() != 2 {
                errors.push(VerifierError::with_location(
                    format!(
                        "Arithmetic operation expects 2 arguments, got {}",
                        inst_data.args.len()
                    ),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.results.len() != 1 {
                errors.push(VerifierError::with_location(
                    format!(
                        "Arithmetic operation expects 1 result, got {}",
                        inst_data.results.len()
                    ),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.block_args.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Arithmetic operation should not have block_args"),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.ty.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Arithmetic operation should not have type"),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.imm.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Arithmetic operation should not have immediate"),
                    format!("inst{}", inst.index()),
                ));
            }
        }

        // Bitwise ops (binary): 2 args, 1 result, no block_args, no ty, no imm
        Opcode::Iand | Opcode::Ior | Opcode::Ixor | Opcode::Ishl | Opcode::Ishr | Opcode::Iashr => {
            if inst_data.args.len() != 2 {
                errors.push(VerifierError::with_location(
                    format!(
                        "Bitwise/shift operation expects 2 arguments, got {}",
                        inst_data.args.len()
                    ),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.results.len() != 1 {
                errors.push(VerifierError::with_location(
                    format!(
                        "Bitwise/shift operation expects 1 result, got {}",
                        inst_data.results.len()
                    ),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.block_args.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Bitwise/shift operation should not have block_args"),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.ty.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Bitwise/shift operation should not have type"),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.imm.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Bitwise/shift operation should not have immediate"),
                    format!("inst{}", inst.index()),
                ));
            }
        }

        // Bitwise NOT (unary): 1 arg, 1 result, no block_args, no ty, no imm
        Opcode::Inot => {
            if inst_data.args.len() != 1 {
                errors.push(VerifierError::with_location(
                    format!(
                        "Bitwise NOT operation expects 1 argument, got {}",
                        inst_data.args.len()
                    ),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.results.len() != 1 {
                errors.push(VerifierError::with_location(
                    format!(
                        "Bitwise NOT operation expects 1 result, got {}",
                        inst_data.results.len()
                    ),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.block_args.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Bitwise NOT operation should not have block_args"),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.ty.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Bitwise NOT operation should not have type"),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.imm.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Bitwise NOT operation should not have immediate"),
                    format!("inst{}", inst.index()),
                ));
            }
        }

        // Integer comparison with condition code: 2 args, 1 result, IntCondCode immediate
        Opcode::Icmp { .. } => {
            if inst_data.args.len() != 2 {
                errors.push(VerifierError::with_location(
                    format!(
                        "Icmp operation expects 2 arguments, got {}",
                        inst_data.args.len()
                    ),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.results.len() != 1 {
                errors.push(VerifierError::with_location(
                    format!(
                        "Icmp operation expects 1 result, got {}",
                        inst_data.results.len()
                    ),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.block_args.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Icmp operation should not have block_args"),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.ty.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Icmp operation should not have type"),
                    format!("inst{}", inst.index()),
                ));
            }
            match &inst_data.imm {
                Some(crate::dfg::Immediate::IntCondCode(_)) => {
                    // Correct immediate type
                }
                Some(_) => {
                    errors.push(VerifierError::with_location(
                        String::from("Icmp operation requires IntCondCode immediate"),
                        format!("inst{}", inst.index()),
                    ));
                }
                None => {
                    errors.push(VerifierError::with_location(
                        String::from("Icmp operation requires IntCondCode immediate"),
                        format!("inst{}", inst.index()),
                    ));
                }
            }
        }

        // Floating point comparison with condition code: 2 args, 1 result, FloatCondCode immediate
        Opcode::Fcmp { .. } => {
            if inst_data.args.len() != 2 {
                errors.push(VerifierError::with_location(
                    format!(
                        "Fcmp operation expects 2 arguments, got {}",
                        inst_data.args.len()
                    ),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.results.len() != 1 {
                errors.push(VerifierError::with_location(
                    format!(
                        "Fcmp operation expects 1 result, got {}",
                        inst_data.results.len()
                    ),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.block_args.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Fcmp operation should not have block_args"),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.ty.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Fcmp operation should not have type"),
                    format!("inst{}", inst.index()),
                ));
            }
            match &inst_data.imm {
                Some(crate::dfg::Immediate::FloatCondCode(_)) => {
                    // Correct immediate type
                }
                Some(_) => {
                    errors.push(VerifierError::with_location(
                        String::from("Fcmp operation requires FloatCondCode immediate"),
                        format!("inst{}", inst.index()),
                    ));
                }
                None => {
                    errors.push(VerifierError::with_location(
                        String::from("Fcmp operation requires FloatCondCode immediate"),
                        format!("inst{}", inst.index()),
                    ));
                }
            }
        }

        // Stack allocation: 0 args, 1 result, no block_args, no ty, no imm (size in opcode)
        Opcode::StackAlloc { .. } => {
            if !inst_data.args.is_empty() {
                errors.push(VerifierError::with_location(
                    format!(
                        "StackAlloc operation expects 0 arguments, got {}",
                        inst_data.args.len()
                    ),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.results.len() != 1 {
                errors.push(VerifierError::with_location(
                    format!(
                        "StackAlloc operation expects 1 result, got {}",
                        inst_data.results.len()
                    ),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.block_args.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("StackAlloc operation should not have block_args"),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.ty.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("StackAlloc operation should not have type"),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.imm.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("StackAlloc operation should not have immediate"),
                    format!("inst{}", inst.index()),
                ));
            }
        }

        // Constants: 0 args, 1 result, imm present, no block_args, no ty
        Opcode::Iconst | Opcode::Fconst => {
            if !inst_data.args.is_empty() {
                errors.push(VerifierError::with_location(
                    format!(
                        "Constant operation expects 0 arguments, got {}",
                        inst_data.args.len()
                    ),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.results.len() != 1 {
                errors.push(VerifierError::with_location(
                    format!(
                        "Constant operation expects 1 result, got {}",
                        inst_data.results.len()
                    ),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.imm.is_none() {
                errors.push(VerifierError::with_location(
                    String::from("Constant operation requires immediate value"),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.block_args.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Constant operation should not have block_args"),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.ty.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Constant operation should not have type"),
                    format!("inst{}", inst.index()),
                ));
            }
        }

        // Jump: args matching block_args targets, 0 results, block_args present with 1 target
        Opcode::Jump => {
            if inst_data.results.len() != 0 {
                errors.push(VerifierError::with_location(
                    format!("Jump expects 0 results, got {}", inst_data.results.len()),
                    format!("inst{}", inst.index()),
                ));
            }
            if let Some(block_args) = &inst_data.block_args {
                if block_args.targets.len() != 1 {
                    errors.push(VerifierError::with_location(
                        format!("Jump expects 1 target, got {}", block_args.targets.len()),
                        format!("inst{}", inst.index()),
                    ));
                } else {
                    let (_target, target_args) = &block_args.targets[0];
                    if inst_data.args.len() != target_args.len() {
                        errors.push(VerifierError::with_location(
                            format!(
                                "Jump args count ({}) does not match target args count ({})",
                                inst_data.args.len(),
                                target_args.len()
                            ),
                            format!("inst{}", inst.index()),
                        ));
                    }
                }
            } else {
                errors.push(VerifierError::with_location(
                    String::from("Jump requires block_args"),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.ty.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Jump should not have type"),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.imm.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Jump should not have immediate"),
                    format!("inst{}", inst.index()),
                ));
            }
        }

        // Br: 1 condition arg + args for both targets, 0 results, block_args present with 2 targets
        Opcode::Br => {
            if inst_data.results.len() != 0 {
                errors.push(VerifierError::with_location(
                    format!("Branch expects 0 results, got {}", inst_data.results.len()),
                    format!("inst{}", inst.index()),
                ));
            }
            if let Some(block_args) = &inst_data.block_args {
                if block_args.targets.len() != 2 {
                    errors.push(VerifierError::with_location(
                        format!("Branch expects 2 targets, got {}", block_args.targets.len()),
                        format!("inst{}", inst.index()),
                    ));
                } else {
                    let (_target_true, args_true) = &block_args.targets[0];
                    let (_target_false, args_false) = &block_args.targets[1];
                    let expected_args = 1 + args_true.len() + args_false.len();
                    if inst_data.args.len() != expected_args {
                        errors.push(VerifierError::with_location(
                            format!(
                                "Branch expects {} arguments (1 condition + {} true args + {} \
                                 false args), got {}",
                                expected_args,
                                args_true.len(),
                                args_false.len(),
                                inst_data.args.len()
                            ),
                            format!("inst{}", inst.index()),
                        ));
                    }
                }
            } else {
                errors.push(VerifierError::with_location(
                    String::from("Branch requires block_args"),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.ty.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Branch should not have type"),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.imm.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Branch should not have immediate"),
                    format!("inst{}", inst.index()),
                ));
            }
        }

        // Return: args count matches function signature, results must be empty
        Opcode::Return => {
            if inst_data.results.len() != 0 {
                errors.push(VerifierError::with_location(
                    format!(
                        "Return instruction must have 0 results, got {}",
                        inst_data.results.len()
                    ),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.args.len() != function.signature.returns.len() {
                errors.push(VerifierError::with_location(
                    format!(
                        "Return expects {} arguments (matching function signature), got {}",
                        function.signature.returns.len(),
                        inst_data.args.len()
                    ),
                    format!("inst{}", inst.index()),
                ));
            }
            // Verify return value types match function signature
            for (i, ret_value) in inst_data.args.iter().enumerate() {
                if let Some(ret_ty) = function.dfg.value_type(*ret_value) {
                    if let Some(expected_ty) = function.signature.returns.get(i) {
                        if ret_ty != *expected_ty {
                            errors.push(VerifierError::with_location(
                                format!(
                                    "Return value {} has type {}, expected {}",
                                    i, ret_ty, expected_ty
                                ),
                                format!("inst{}", inst.index()),
                            ));
                        }
                    }
                }
            }
            if inst_data.block_args.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Return should not have block_args"),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.ty.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Return should not have type"),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.imm.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Return should not have immediate"),
                    format!("inst{}", inst.index()),
                ));
            }
        }

        // Load: 1 arg (address), 1 result, ty present, no block_args
        Opcode::Load => {
            if inst_data.args.len() != 1 {
                errors.push(VerifierError::with_location(
                    format!(
                        "Load expects 1 argument (address), got {}",
                        inst_data.args.len()
                    ),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.results.len() != 1 {
                errors.push(VerifierError::with_location(
                    format!("Load expects 1 result, got {}", inst_data.results.len()),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.ty.is_none() {
                errors.push(VerifierError::with_location(
                    String::from("Load requires type information"),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.block_args.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Load should not have block_args"),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.imm.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Load should not have immediate"),
                    format!("inst{}", inst.index()),
                ));
            }
        }

        // Store: 2 args (address, value), 0 results, ty present, no block_args
        Opcode::Store => {
            if inst_data.args.len() != 2 {
                errors.push(VerifierError::with_location(
                    format!(
                        "Store expects 2 arguments (address, value), got {}",
                        inst_data.args.len()
                    ),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.results.len() != 0 {
                errors.push(VerifierError::with_location(
                    format!("Store expects 0 results, got {}", inst_data.results.len()),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.ty.is_none() {
                errors.push(VerifierError::with_location(
                    String::from("Store requires type information"),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.block_args.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Store should not have block_args"),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.imm.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Store should not have immediate"),
                    format!("inst{}", inst.index()),
                ));
            }
        }

        // Call: args/results matching signature (when available), no block_args
        Opcode::Call { .. } => {
            if inst_data.block_args.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Call should not have block_args"),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.ty.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Call should not have type"),
                    format!("inst{}", inst.index()),
                ));
            }
            // Note: Call signature validation is done in type checking
        }

        // Syscall: imm present (syscall number), no results, no block_args
        Opcode::Syscall => {
            if inst_data.results.len() != 0 {
                errors.push(VerifierError::with_location(
                    format!("Syscall expects 0 results, got {}", inst_data.results.len()),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.imm.is_none() {
                errors.push(VerifierError::with_location(
                    String::from("Syscall requires immediate value (syscall number)"),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.block_args.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Syscall should not have block_args"),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.ty.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Syscall should not have type"),
                    format!("inst{}", inst.index()),
                ));
            }
        }

        // Halt: 0 args, 0 results, no block_args, no ty, no imm
        Opcode::Halt => {
            if inst_data.args.len() != 0 {
                errors.push(VerifierError::with_location(
                    format!("Halt expects 0 arguments, got {}", inst_data.args.len()),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.results.len() != 0 {
                errors.push(VerifierError::with_location(
                    format!("Halt expects 0 results, got {}", inst_data.results.len()),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.block_args.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Halt should not have block_args"),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.ty.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Halt should not have type"),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.imm.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Halt should not have immediate"),
                    format!("inst{}", inst.index()),
                ));
            }
        }

        // Trap: 0 args, 0 results, TrapCode immediate, no block_args, no ty
        Opcode::Trap { .. } => {
            if inst_data.args.len() != 0 {
                errors.push(VerifierError::with_location(
                    format!("Trap expects 0 arguments, got {}", inst_data.args.len()),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.results.len() != 0 {
                errors.push(VerifierError::with_location(
                    format!("Trap expects 0 results, got {}", inst_data.results.len()),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.block_args.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Trap should not have block_args"),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.ty.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Trap should not have type"),
                    format!("inst{}", inst.index()),
                ));
            }
            match &inst_data.imm {
                Some(crate::dfg::Immediate::TrapCode(_)) => {
                    // Correct immediate type
                }
                Some(_) => {
                    errors.push(VerifierError::with_location(
                        String::from("Trap operation requires TrapCode immediate"),
                        format!("inst{}", inst.index()),
                    ));
                }
                None => {
                    errors.push(VerifierError::with_location(
                        String::from("Trap operation requires TrapCode immediate"),
                        format!("inst{}", inst.index()),
                    ));
                }
            }
        }

        // Trapz: 1 arg (condition), 0 results, TrapCode immediate, no block_args, no ty
        Opcode::Trapz { .. } => {
            if inst_data.args.len() != 1 {
                errors.push(VerifierError::with_location(
                    format!("Trapz expects 1 argument, got {}", inst_data.args.len()),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.results.len() != 0 {
                errors.push(VerifierError::with_location(
                    format!("Trapz expects 0 results, got {}", inst_data.results.len()),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.block_args.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Trapz should not have block_args"),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.ty.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Trapz should not have type"),
                    format!("inst{}", inst.index()),
                ));
            }
            match &inst_data.imm {
                Some(crate::dfg::Immediate::TrapCode(_)) => {
                    // Correct immediate type
                }
                Some(_) => {
                    errors.push(VerifierError::with_location(
                        String::from("Trapz operation requires TrapCode immediate"),
                        format!("inst{}", inst.index()),
                    ));
                }
                None => {
                    errors.push(VerifierError::with_location(
                        String::from("Trapz operation requires TrapCode immediate"),
                        format!("inst{}", inst.index()),
                    ));
                }
            }
        }

        // Trapnz: 1 arg (condition), 0 results, TrapCode immediate, no block_args, no ty
        Opcode::Trapnz { .. } => {
            if inst_data.args.len() != 1 {
                errors.push(VerifierError::with_location(
                    format!("Trapnz expects 1 argument, got {}", inst_data.args.len()),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.results.len() != 0 {
                errors.push(VerifierError::with_location(
                    format!("Trapnz expects 0 results, got {}", inst_data.results.len()),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.block_args.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Trapnz should not have block_args"),
                    format!("inst{}", inst.index()),
                ));
            }
            if inst_data.ty.is_some() {
                errors.push(VerifierError::with_location(
                    String::from("Trapnz should not have type"),
                    format!("inst{}", inst.index()),
                ));
            }
            match &inst_data.imm {
                Some(crate::dfg::Immediate::TrapCode(_)) => {
                    // Correct immediate type
                }
                Some(_) => {
                    errors.push(VerifierError::with_location(
                        String::from("Trapnz operation requires TrapCode immediate"),
                        format!("inst{}", inst.index()),
                    ));
                }
                None => {
                    errors.push(VerifierError::with_location(
                        String::from("Trapnz operation requires TrapCode immediate"),
                        format!("inst{}", inst.index()),
                    ));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::{string::String, vec};

    use super::*;
    use crate::{
        dfg::{InstData, Opcode},
        signature::Signature,
        types::Type,
        value::Value,
    };

    #[test]
    fn test_verify_format_arithmetic_valid() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let block = func.create_block();
        func.append_block(block);

        let v1 = Value::new(1);
        let v2 = Value::new(2);
        let v3 = Value::new(3);
        let inst_data = InstData::arithmetic(Opcode::Iadd, v3, v1, v2);
        let inst = func.create_inst(inst_data);
        func.append_inst(inst, block);

        let mut errors = Vec::new();
        verify_format(&func, &mut errors);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_verify_format_arithmetic_wrong_args() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let block = func.create_block();
        func.append_block(block);

        let v1 = Value::new(1);
        let v2 = Value::new(2);
        let v3 = Value::new(3);
        let mut inst_data = InstData::arithmetic(Opcode::Iadd, v3, v1, v2);
        inst_data.args.push(Value::new(4)); // Wrong number of args
        let inst = func.create_inst(inst_data);
        func.append_inst(inst, block);

        let mut errors = Vec::new();
        verify_format(&func, &mut errors);
        assert!(!errors.is_empty());
        assert!(errors[0].message.contains("expects 2 arguments"));
    }

    #[test]
    fn test_verify_format_constant_missing_imm() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let block = func.create_block();
        func.append_block(block);

        let v1 = Value::new(1);
        let mut inst_data = InstData::constant(v1, crate::dfg::Immediate::I64(42));
        inst_data.imm = None; // Remove immediate
        let inst = func.create_inst(inst_data);
        func.append_inst(inst, block);

        let mut errors = Vec::new();
        verify_format(&func, &mut errors);
        assert!(!errors.is_empty());
        assert!(errors[0].message.contains("requires immediate value"));
    }

    #[test]
    fn test_verify_format_load_missing_type() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let block = func.create_block();
        func.append_block(block);

        let v1 = Value::new(1);
        let v2 = Value::new(2);
        let mut inst_data = InstData::load(v2, v1, Type::I32);
        inst_data.ty = None; // Remove type
        let inst = func.create_inst(inst_data);
        func.append_inst(inst, block);

        let mut errors = Vec::new();
        verify_format(&func, &mut errors);
        assert!(!errors.is_empty());
        assert!(errors[0].message.contains("requires type information"));
    }

    #[test]
    fn test_verify_format_jump_missing_block_args() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let block0 = func.create_block();
        let block1 = func.create_block();
        func.append_block(block0);
        func.append_block(block1);

        let mut inst_data = InstData::jump(block1, vec![]);
        inst_data.block_args = None; // Remove block_args
        let inst = func.create_inst(inst_data);
        func.append_inst(inst, block0);

        let mut errors = Vec::new();
        verify_format(&func, &mut errors);
        assert!(!errors.is_empty());
        assert!(errors[0].message.contains("requires block_args"));
    }

    #[test]
    fn test_verify_format_syscall_missing_imm() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let block = func.create_block();
        func.append_block(block);

        let mut inst_data = InstData::syscall(1, vec![], vec![]);
        inst_data.imm = None; // Remove immediate
        let inst = func.create_inst(inst_data);
        func.append_inst(inst, block);

        let mut errors = Vec::new();
        verify_format(&func, &mut errors);
        assert!(!errors.is_empty());
        assert!(errors[0].message.contains("requires immediate value"));
    }

    #[test]
    fn test_verify_format_return_multi_return() {
        let sig = Signature::new(vec![], vec![Type::I32, Type::F32, Type::I32]);
        let mut func = Function::new(sig, String::from("test"));
        let block = func.create_block();
        func.append_block(block);

        let v1 = Value::new(1);
        let v2 = Value::new(2);
        let v3 = Value::new(3);
        func.dfg.set_value_type(v1, Type::I32);
        func.dfg.set_value_type(v2, Type::F32);
        func.dfg.set_value_type(v3, Type::I32);

        let inst_data = InstData::return_(vec![v1, v2, v3]);
        let inst = func.create_inst(inst_data);
        func.append_inst(inst, block);

        let mut errors = Vec::new();
        verify_format(&func, &mut errors);
        assert!(
            errors.is_empty(),
            "Multi-return with matching signature should pass"
        );
    }

    #[test]
    fn test_verify_format_return_wrong_count() {
        let sig = Signature::new(vec![], vec![Type::I32, Type::F32]);
        let mut func = Function::new(sig, String::from("test"));
        let block = func.create_block();
        func.append_block(block);

        let v1 = Value::new(1);
        func.dfg.set_value_type(v1, Type::I32);

        // Return with wrong count (1 instead of 2)
        let inst_data = InstData::return_(vec![v1]);
        let inst = func.create_inst(inst_data);
        func.append_inst(inst, block);

        let mut errors = Vec::new();
        verify_format(&func, &mut errors);
        assert!(!errors.is_empty());
        assert!(errors
            .iter()
            .any(|e| e.message.contains("expects 2 arguments")));
    }

    #[test]
    fn test_verify_format_return_wrong_types() {
        let sig = Signature::new(vec![], vec![Type::I32, Type::F32]);
        let mut func = Function::new(sig, String::from("test"));
        let block = func.create_block();
        func.append_block(block);

        let v1 = Value::new(1);
        let v2 = Value::new(2);
        func.dfg.set_value_type(v1, Type::I32);
        func.dfg.set_value_type(v2, Type::I32); // Wrong type, should be F32

        let inst_data = InstData::return_(vec![v1, v2]);
        let inst = func.create_inst(inst_data);
        func.append_inst(inst, block);

        let mut errors = Vec::new();
        verify_format(&func, &mut errors);
        assert!(!errors.is_empty());
        assert!(errors
            .iter()
            .any(|e| e.message.contains("has type i32, expected f32")));
    }

    #[test]
    fn test_verify_format_return_void() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let block = func.create_block();
        func.append_block(block);

        let inst_data = InstData::return_(vec![]);
        let inst = func.create_inst(inst_data);
        func.append_inst(inst, block);

        let mut errors = Vec::new();
        verify_format(&func, &mut errors);
        assert!(
            errors.is_empty(),
            "Return with 0 values for void function should pass"
        );
    }

    #[test]
    fn test_verify_format_return_with_results() {
        let sig = Signature::new(vec![], vec![Type::I32]);
        let mut func = Function::new(sig, String::from("test"));
        let block = func.create_block();
        func.append_block(block);

        let v1 = Value::new(1);
        func.dfg.set_value_type(v1, Type::I32);

        let mut inst_data = InstData::return_(vec![v1]);
        inst_data.results.push(Value::new(999)); // Incorrectly add a result
        let inst = func.create_inst(inst_data);
        func.append_inst(inst, block);

        let mut errors = Vec::new();
        verify_format(&func, &mut errors);
        assert!(!errors.is_empty());
        assert!(errors
            .iter()
            .any(|e| e.message.contains("must have 0 results")));
    }
}
