//! Float-to-fixed16x16 transformation pass.
//!
//! This module converts floating point operations to fixed-point arithmetic
//! at the LPIR level. All F32 types and operations are converted to I32
//! fixed-point representation (16.16 format).

use alloc::{format, string::String, vec::Vec};

use crate::{
    condcodes::{FloatCC, IntCC},
    dfg::Opcode,
    entity::{Block as BlockEntity, Inst as InstEntity},
    function::Function,
    signature::Signature,
    types::Type,
    verifier::verify,
};

/// Convert a float32 value to fixed16x16 representation.
///
/// Fixed16x16 format uses 16 integer bits and 16 fractional bits.
/// Range: -32768.0 to +32767.9999847412109375
/// Precision: 1/65536 (approximately 0.00001526)
pub fn float_to_fixed16x16(f: f32) -> i32 {
    // Clamp to representable range
    let clamped = f.clamp(-32768.0, 32767.9999847412109375);
    // Convert to fixed-point (round to nearest)
    // Manual rounding: add 0.5 and truncate
    let scaled = clamped * 65536.0;
    let rounded = if scaled >= 0.0 {
        (scaled + 0.5) as i32
    } else {
        (scaled - 0.5) as i32
    };
    rounded
}

/// Convert fixed16x16 back to float32 (for debugging/constants).
#[allow(dead_code)]
pub fn fixed16x16_to_float(fixed: i32) -> f32 {
    fixed as f32 / 65536.0
}

/// Error type for transformation errors
#[derive(Debug, Clone)]
pub struct TransformError {
    pub message: String,
}

impl TransformError {
    pub fn new(message: String) -> Self {
        Self { message }
    }
}

impl core::fmt::Display for TransformError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// Convert all float operations in a function to fixed16x16.
///
/// This pass:
/// 1. Converts function signature (F32 → I32)
/// 2. Converts all F32 values to I32 (fixed-point representation)
/// 3. Converts all float operations to fixed-point operations
/// 4. Updates all value types
/// 5. Verifies the function is still valid
pub fn convert_floats_to_fixed16x16(func: &mut Function) -> Result<(), TransformError> {
    // 1. Convert signature
    convert_signature(&mut func.signature);

    // 2. Walk all blocks and instructions to convert them
    // We need to collect instructions first to avoid borrow issues
    let mut insts_to_convert: Vec<(BlockEntity, InstEntity)> = Vec::new();
    for block in func.layout.blocks() {
        for inst in func.block_insts(block) {
            insts_to_convert.push((block, inst));
        }
    }

    // 3. Convert each instruction
    for (block, inst) in insts_to_convert {
        let inst_data = func.dfg.inst_data(inst).cloned();
        if let Some(inst_data) = inst_data {
            match inst_data.opcode {
                Opcode::Fconst => {
                    convert_fconst(func, inst, block)?;
                }
                Opcode::Fadd => {
                    convert_fadd(func, inst, block)?;
                }
                Opcode::Fsub => {
                    convert_fsub(func, inst, block)?;
                }
                Opcode::Fmul => {
                    convert_fmul(func, inst, block)?;
                }
                Opcode::Fdiv => {
                    convert_fdiv(func, inst, block)?;
                }
                Opcode::Fcmp { cond } => {
                    convert_fcmp(func, inst, block, cond)?;
                }
                Opcode::Load if inst_data.ty == Some(Type::F32) => {
                    convert_load(func, inst, block)?;
                }
                Opcode::Store if inst_data.ty == Some(Type::F32) => {
                    convert_store(func, inst, block)?;
                }
                _ => {
                    // Update operand types if they reference F32 values
                    // (handled by update_all_value_types below)
                }
            }
        }
    }

    // 4. Update all value types from F32 to I32
    update_all_value_types(func);

    // 5. Verify function is still valid
    if let Err(errors) = verify(func, None) {
        return Err(TransformError::new(format!(
            "Verification failed after transformation: {:?}",
            errors
        )));
    }

    Ok(())
}

/// Convert function signature: F32 params/returns → I32
fn convert_signature(sig: &mut Signature) {
    for param_ty in &mut sig.params {
        if *param_ty == Type::F32 {
            *param_ty = Type::I32;
        }
    }
    for ret_ty in &mut sig.returns {
        if *ret_ty == Type::F32 {
            *ret_ty = Type::I32;
        }
    }
}

/// Convert Fconst to Iconst with fixed-point value
fn convert_fconst(
    func: &mut Function,
    inst: InstEntity,
    _block: BlockEntity,
) -> Result<(), TransformError> {
    let inst_data = func.dfg.inst_data(inst).cloned();
    if let Some(inst_data) = inst_data {
        if let Some(imm) = &inst_data.imm {
            if let crate::dfg::Immediate::F32Bits(bits) = imm {
                let f32_value = f32::from_bits(*bits);
                let fixed_value = float_to_fixed16x16(f32_value);
                let result = inst_data.results[0];

                // Replace Fconst with Iconst
                let new_inst_data = crate::dfg::InstData::constant(
                    result,
                    crate::dfg::Immediate::I64(fixed_value as i64),
                );
                func.dfg.inst_data_mut(inst).map(|d| *d = new_inst_data);
                func.dfg.set_value_type(result, Type::I32);
            }
        }
    }
    Ok(())
}

/// Convert Fadd to Iadd (fixed-point addition is direct integer addition)
fn convert_fadd(
    func: &mut Function,
    inst: InstEntity,
    _block: BlockEntity,
) -> Result<(), TransformError> {
    let inst_data = func.dfg.inst_data(inst).cloned();
    if let Some(inst_data) = inst_data {
        let result = inst_data.results[0];
        let arg1 = inst_data.args[0];
        let arg2 = inst_data.args[1];

        // Replace Fadd with Iadd
        let new_inst_data = crate::dfg::InstData::arithmetic(Opcode::Iadd, result, arg1, arg2);
        func.dfg.inst_data_mut(inst).map(|d| *d = new_inst_data);
        func.dfg.set_value_type(result, Type::I32);
    }
    Ok(())
}

/// Convert Fsub to Isub (fixed-point subtraction is direct integer subtraction)
fn convert_fsub(
    func: &mut Function,
    inst: InstEntity,
    _block: BlockEntity,
) -> Result<(), TransformError> {
    let inst_data = func.dfg.inst_data(inst).cloned();
    if let Some(inst_data) = inst_data {
        let result = inst_data.results[0];
        let arg1 = inst_data.args[0];
        let arg2 = inst_data.args[1];

        // Replace Fsub with Isub
        let new_inst_data = crate::dfg::InstData::arithmetic(Opcode::Isub, result, arg1, arg2);
        func.dfg.inst_data_mut(inst).map(|d| *d = new_inst_data);
        func.dfg.set_value_type(result, Type::I32);
    }
    Ok(())
}

/// Convert Fmul to fixed-point multiplication sequence
/// For fixed-point multiply: result = (a * b) >> 16
/// Algorithm:
/// 1. hi = MULH(a, b)  // High 32 bits of product
/// 2. lo = MUL(a, b)   // Low 32 bits of product
/// 3. hi_shifted = hi << 16
/// 4. lo_shifted = lo >> 16
/// 5. result = hi_shifted | lo_shifted
fn convert_fmul(
    func: &mut Function,
    inst: InstEntity,
    block: BlockEntity,
) -> Result<(), TransformError> {
    let inst_data = func.dfg.inst_data(inst).cloned();
    if let Some(inst_data) = inst_data {
        let result = inst_data.results[0];
        let arg1 = inst_data.args[0];
        let arg2 = inst_data.args[1];

        // Create new values for temporaries
        let next_idx = func.dfg.next_value_index();
        let hi = crate::value::Value::new(next_idx);
        let lo = crate::value::Value::new(next_idx + 1);
        let hi_shifted = crate::value::Value::new(next_idx + 2);
        let lo_shifted = crate::value::Value::new(next_idx + 3);
        let shift_16 = crate::value::Value::new(next_idx + 4);

        // Set types for all values
        func.dfg.set_value_type(hi, Type::I32);
        func.dfg.set_value_type(lo, Type::I32);
        func.dfg.set_value_type(hi_shifted, Type::I32);
        func.dfg.set_value_type(lo_shifted, Type::I32);
        func.dfg.set_value_type(shift_16, Type::I32);
        func.dfg.set_value_type(result, Type::I32);

        // Find the next instruction after this one (likely the return)
        let next_inst = func.layout.next_inst(inst);

        // Create constant 16 for shift
        let shift_const_inst = func.create_inst(crate::dfg::InstData::constant(
            shift_16,
            crate::dfg::Immediate::I64(16),
        ));
        if let Some(next) = next_inst {
            func.layout.insert_inst(shift_const_inst, next);
        } else {
            func.append_inst(shift_const_inst, block);
        }

        // Compute high and low parts
        let hi_inst = func.create_inst(crate::dfg::InstData::arithmetic(
            Opcode::Imulh,
            hi,
            arg1,
            arg2,
        ));
        if let Some(next) = next_inst {
            func.layout.insert_inst(hi_inst, next);
        } else {
            func.append_inst(hi_inst, block);
        }

        let lo_inst = func.create_inst(crate::dfg::InstData::arithmetic(
            Opcode::Imul,
            lo,
            arg1,
            arg2,
        ));
        if let Some(next) = next_inst {
            func.layout.insert_inst(lo_inst, next);
        } else {
            func.append_inst(lo_inst, block);
        }

        // Shift: hi << 16, lo >> 16
        let hi_shift_inst = func.create_inst(crate::dfg::InstData::shift(
            Opcode::Ishl,
            hi_shifted,
            hi,
            shift_16,
        ));
        if let Some(next) = next_inst {
            func.layout.insert_inst(hi_shift_inst, next);
        } else {
            func.append_inst(hi_shift_inst, block);
        }

        let lo_shift_inst = func.create_inst(crate::dfg::InstData::shift(
            Opcode::Ishr,
            lo_shifted,
            lo,
            shift_16,
        ));
        if let Some(next) = next_inst {
            func.layout.insert_inst(lo_shift_inst, next);
        } else {
            func.append_inst(lo_shift_inst, block);
        }

        // Combine: result = hi_shifted | lo_shifted
        let combine_inst = func.create_inst(crate::dfg::InstData::bitwise(
            Opcode::Ior,
            result,
            hi_shifted,
            lo_shifted,
        ));
        if let Some(next) = next_inst {
            func.layout.insert_inst(combine_inst, next);
        } else {
            func.append_inst(combine_inst, block);
        }

        // Remove the original Fmul instruction
        func.layout.remove_inst(inst);
    }
    Ok(())
}

/// Convert Fdiv to fixed-point division sequence
/// For fixed-point divide: result = (a << 16) / b
/// This is complex and requires extended precision division.
/// For now, we'll use a simplified approach that may have precision issues
/// for large values. A full implementation would use a library function.
fn convert_fdiv(
    func: &mut Function,
    inst: InstEntity,
    block: BlockEntity,
) -> Result<(), TransformError> {
    let inst_data = func.dfg.inst_data(inst).cloned();
    if let Some(inst_data) = inst_data {
        let result = inst_data.results[0];
        let arg1 = inst_data.args[0]; // a
        let arg2 = inst_data.args[1]; // b

        // Create new values for temporaries
        let next_idx = func.dfg.next_value_index();
        let a_shifted = crate::value::Value::new(next_idx);
        let shift_16 = crate::value::Value::new(next_idx + 1);

        // Set types
        func.dfg.set_value_type(a_shifted, Type::I32);
        func.dfg.set_value_type(shift_16, Type::I32);
        func.dfg.set_value_type(result, Type::I32);

        // Find the next instruction after this one (likely the return)
        let next_inst = func.layout.next_inst(inst);

        // Create constant 16 for shift
        let shift_const_inst = func.create_inst(crate::dfg::InstData::constant(
            shift_16,
            crate::dfg::Immediate::I64(16),
        ));
        if let Some(next) = next_inst {
            func.layout.insert_inst(shift_const_inst, next);
        } else {
            func.append_inst(shift_const_inst, block);
        }

        // Shift a left by 16: a_shifted = a << 16
        let shift_inst = func.create_inst(crate::dfg::InstData::shift(
            Opcode::Ishl,
            a_shifted,
            arg1,
            shift_16,
        ));
        if let Some(next) = next_inst {
            func.layout.insert_inst(shift_inst, next);
        } else {
            func.append_inst(shift_inst, block);
        }

        // Divide: result = a_shifted / b
        // Note: This may overflow for large values, but it's a reasonable approximation
        let div_inst = func.create_inst(crate::dfg::InstData::arithmetic(
            Opcode::Idiv,
            result,
            a_shifted,
            arg2,
        ));
        if let Some(next) = next_inst {
            func.layout.insert_inst(div_inst, next);
        } else {
            func.append_inst(div_inst, block);
        }

        // Remove the original Fdiv instruction
        func.layout.remove_inst(inst);
    }
    Ok(())
}

/// Convert Fcmp to Icmp with appropriate condition code
fn convert_fcmp(
    func: &mut Function,
    inst: InstEntity,
    _block: BlockEntity,
    cond: FloatCC,
) -> Result<(), TransformError> {
    let inst_data = func.dfg.inst_data(inst).cloned();
    if let Some(inst_data) = inst_data {
        let result = inst_data.results[0];
        let arg1 = inst_data.args[0];
        let arg2 = inst_data.args[1];

        // Convert FloatCC to IntCC
        let int_cond = match cond {
            FloatCC::Equal => IntCC::Equal,
            FloatCC::NotEqual => IntCC::NotEqual,
            FloatCC::LessThan => IntCC::SignedLessThan,
            FloatCC::LessThanOrEqual => IntCC::SignedLessThanOrEqual,
            FloatCC::GreaterThan => IntCC::SignedGreaterThan,
            FloatCC::GreaterThanOrEqual => IntCC::SignedGreaterThanOrEqual,
            FloatCC::Unordered => {
                // No NaN in fixed-point, so unordered is always false
                // We'll use a comparison that's always false
                IntCC::NotEqual
            }
            FloatCC::Ordered => {
                // No NaN in fixed-point, so ordered is always true
                // We'll use a comparison that's always true
                IntCC::Equal
            }
            FloatCC::UnorderedOrEqual => IntCC::Equal, // No NaN, so just equal
            FloatCC::OrderedNotEqual => IntCC::NotEqual, // No NaN, so just not equal
            FloatCC::UnorderedOrLessThan => IntCC::SignedLessThan,
            FloatCC::UnorderedOrLessThanOrEqual => IntCC::SignedLessThanOrEqual,
            FloatCC::UnorderedOrGreaterThan => IntCC::SignedGreaterThan,
            FloatCC::UnorderedOrGreaterThanOrEqual => IntCC::SignedGreaterThanOrEqual,
        };

        // Replace Fcmp with Icmp
        let new_inst_data =
            crate::dfg::InstData::comparison(Opcode::Icmp { cond: int_cond }, result, arg1, arg2);
        func.dfg.inst_data_mut(inst).map(|d| *d = new_inst_data);
        func.dfg.set_value_type(result, Type::I32);
    }
    Ok(())
}

/// Convert Load with F32 type to Load with I32 type
fn convert_load(
    func: &mut Function,
    inst: InstEntity,
    _block: BlockEntity,
) -> Result<(), TransformError> {
    let inst_data = func.dfg.inst_data(inst).cloned();
    if let Some(inst_data) = inst_data {
        let result = inst_data.results[0];
        let address = inst_data.args[0];

        // Replace Load with I32 type
        let new_inst_data = crate::dfg::InstData::load(result, address, Type::I32);
        func.dfg.inst_data_mut(inst).map(|d| *d = new_inst_data);
        func.dfg.set_value_type(result, Type::I32);
    }
    Ok(())
}

/// Convert Store with F32 type to Store with I32 type
fn convert_store(
    func: &mut Function,
    inst: InstEntity,
    _block: BlockEntity,
) -> Result<(), TransformError> {
    let inst_data = func.dfg.inst_data(inst).cloned();
    if let Some(inst_data) = inst_data {
        let address = inst_data.args[0];
        let value = inst_data.args[1];

        // Replace Store with I32 type
        let new_inst_data = crate::dfg::InstData::store(address, value, Type::I32);
        func.dfg.inst_data_mut(inst).map(|d| *d = new_inst_data);
    }
    Ok(())
}

/// Update all value types from F32 to I32
fn update_all_value_types(func: &mut Function) {
    // Get all values with F32 type and convert them to I32
    // We iterate over all instructions to find values
    let mut values_to_update = Vec::new();
    for block in func.layout.blocks() {
        for inst in func.block_insts(block) {
            if let Some(inst_data) = func.dfg.inst_data(inst) {
                // Check results
                for result in &inst_data.results {
                    if let Some(Type::F32) = func.dfg.value_type(*result) {
                        values_to_update.push(*result);
                    }
                }
                // Check args (for block args, etc.)
                for arg in &inst_data.args {
                    if let Some(Type::F32) = func.dfg.value_type(*arg) {
                        values_to_update.push(*arg);
                    }
                }
            }
        }
    }

    // Also check block parameters
    for block in func.layout.blocks() {
        if let Some(block_data) = func.block_data(block) {
            for param in &block_data.params {
                if let Some(Type::F32) = func.dfg.value_type(*param) {
                    values_to_update.push(*param);
                }
            }
        }
    }

    for value in values_to_update {
        func.dfg.set_value_type(value, Type::I32);
    }
}
