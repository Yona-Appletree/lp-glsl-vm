//! Instruction parsers.

use nom::{
    branch::alt,
    bytes::complete::tag,
    character::complete::char,
    combinator::{map, opt, peek},
    multi::separated_list0,
    sequence::{delimited, preceded, terminated, tuple},
    IResult,
};

use super::{
    primitives::{float, integer, parse_block_index, parse_function_name, parse_type, parse_value},
    whitespace::blank,
};
use crate::{
    condcodes::{FloatCC, IntCC},
    dfg::{Immediate, InstData, Opcode},
    entity::{Block, EntityRef},
    trapcode::TrapCode,
};

/// Parse an arithmetic instruction (iadd, isub, imul, imulh, idiv, irem)
pub(crate) fn parse_arithmetic(input: &str) -> IResult<&str, InstData> {
    let (input, result) = terminated(parse_value, blank)(input)?;
    let (input, _) = terminated(tag("="), blank)(input)?;
    let (input, op) = terminated(
        alt((
            tag("iadd"),
            tag("isub"),
            tag("imul"),
            tag("imulh"),
            tag("idiv"),
            tag("irem"),
        )),
        blank,
    )(input)?;
    let (input, arg1) = terminated(parse_value, blank)(input)?;
    let (input, _) = terminated(char(','), blank)(input)?;
    let (input, arg2) = terminated(parse_value, blank)(input)?;

    let opcode = match op {
        "iadd" => Opcode::Iadd,
        "isub" => Opcode::Isub,
        "imul" => Opcode::Imul,
        "imulh" => Opcode::Imulh,
        "idiv" => Opcode::Idiv,
        "irem" => Opcode::Irem,
        _ => unreachable!(),
    };

    Ok((input, InstData::arithmetic(opcode, result, arg1, arg2)))
}

/// Parse a floating point arithmetic instruction (fadd, fsub, fmul, fdiv)
pub(crate) fn parse_float_arithmetic(input: &str) -> IResult<&str, InstData> {
    let (input, result) = terminated(parse_value, blank)(input)?;
    let (input, _) = terminated(tag("="), blank)(input)?;
    let (input, op) = terminated(
        alt((tag("fadd"), tag("fsub"), tag("fmul"), tag("fdiv"))),
        blank,
    )(input)?;
    let (input, arg1) = terminated(parse_value, blank)(input)?;
    let (input, _) = terminated(char(','), blank)(input)?;
    let (input, arg2) = terminated(parse_value, blank)(input)?;

    let opcode = match op {
        "fadd" => Opcode::Fadd,
        "fsub" => Opcode::Fsub,
        "fmul" => Opcode::Fmul,
        "fdiv" => Opcode::Fdiv,
        _ => unreachable!(),
    };

    Ok((input, InstData::arithmetic(opcode, result, arg1, arg2)))
}

/// Parse a bitwise instruction (iand, ior, ixor)
pub(crate) fn parse_bitwise(input: &str) -> IResult<&str, InstData> {
    let (input, result) = terminated(parse_value, blank)(input)?;
    let (input, _) = terminated(tag("="), blank)(input)?;
    let (input, op) = terminated(alt((tag("iand"), tag("ior"), tag("ixor"))), blank)(input)?;
    let (input, arg1) = terminated(parse_value, blank)(input)?;
    let (input, _) = terminated(char(','), blank)(input)?;
    let (input, arg2) = terminated(parse_value, blank)(input)?;

    let opcode = match op {
        "iand" => Opcode::Iand,
        "ior" => Opcode::Ior,
        "ixor" => Opcode::Ixor,
        _ => unreachable!(),
    };

    Ok((input, InstData::bitwise(opcode, result, arg1, arg2)))
}

/// Parse a bitwise unary instruction (inot)
pub(crate) fn parse_bitwise_unary(input: &str) -> IResult<&str, InstData> {
    let (input, result) = terminated(parse_value, blank)(input)?;
    let (input, _) = terminated(tag("="), blank)(input)?;
    let (input, _) = terminated(tag("inot"), blank)(input)?;
    let (input, arg) = terminated(parse_value, blank)(input)?;

    Ok((input, InstData::bitwise_unary(Opcode::Inot, result, arg)))
}

/// Parse a shift instruction (ishl, ishr, iashr)
pub(crate) fn parse_shift(input: &str) -> IResult<&str, InstData> {
    let (input, result) = terminated(parse_value, blank)(input)?;
    let (input, _) = terminated(tag("="), blank)(input)?;
    let (input, op) = terminated(alt((tag("ishl"), tag("ishr"), tag("iashr"))), blank)(input)?;
    let (input, arg1) = terminated(parse_value, blank)(input)?;
    let (input, _) = terminated(char(','), blank)(input)?;
    let (input, arg2) = terminated(parse_value, blank)(input)?;

    let opcode = match op {
        "ishl" => Opcode::Ishl,
        "ishr" => Opcode::Ishr,
        "iashr" => Opcode::Iashr,
        _ => unreachable!(),
    };

    Ok((input, InstData::shift(opcode, result, arg1, arg2)))
}

/// Parse an integer condition code
fn parse_int_cond_code(input: &str) -> IResult<&str, IntCC> {
    let (input, cond_str) = terminated(
        alt((
            tag("eq"),
            tag("ne"),
            tag("slt"),
            tag("sge"),
            tag("sgt"),
            tag("sle"),
            tag("ult"),
            tag("uge"),
            tag("ugt"),
            tag("ule"),
        )),
        blank,
    )(input)?;

    let cond = match cond_str {
        "eq" => IntCC::Equal,
        "ne" => IntCC::NotEqual,
        "slt" => IntCC::SignedLessThan,
        "sge" => IntCC::SignedGreaterThanOrEqual,
        "sgt" => IntCC::SignedGreaterThan,
        "sle" => IntCC::SignedLessThanOrEqual,
        "ult" => IntCC::UnsignedLessThan,
        "uge" => IntCC::UnsignedGreaterThanOrEqual,
        "ugt" => IntCC::UnsignedGreaterThan,
        "ule" => IntCC::UnsignedLessThanOrEqual,
        _ => unreachable!(),
    };

    Ok((input, cond))
}

/// Parse a floating point condition code
fn parse_float_cond_code(input: &str) -> IResult<&str, FloatCC> {
    let (input, cond_str) = terminated(
        alt((
            tag("ord"),
            tag("uno"),
            tag("eq"),
            tag("ne"),
            tag("one"),
            tag("ueq"),
            tag("lt"),
            tag("le"),
            tag("gt"),
            tag("ge"),
            tag("ult"),
            tag("ule"),
            tag("ugt"),
            tag("uge"),
        )),
        blank,
    )(input)?;

    let cond = match cond_str {
        "ord" => FloatCC::Ordered,
        "uno" => FloatCC::Unordered,
        "eq" => FloatCC::Equal,
        "ne" => FloatCC::NotEqual,
        "one" => FloatCC::OrderedNotEqual,
        "ueq" => FloatCC::UnorderedOrEqual,
        "lt" => FloatCC::LessThan,
        "le" => FloatCC::LessThanOrEqual,
        "gt" => FloatCC::GreaterThan,
        "ge" => FloatCC::GreaterThanOrEqual,
        "ult" => FloatCC::UnorderedOrLessThan,
        "ule" => FloatCC::UnorderedOrLessThanOrEqual,
        "ugt" => FloatCC::UnorderedOrGreaterThan,
        "uge" => FloatCC::UnorderedOrGreaterThanOrEqual,
        _ => unreachable!(),
    };

    Ok((input, cond))
}

/// Parse a trap code
fn parse_trap_code(input: &str) -> IResult<&str, TrapCode> {
    // Try standard trap codes first
    let (input, code) = match terminated(
        alt((
            tag("stk_ovf"),
            tag("heap_oob"),
            tag("int_ovf"),
            tag("int_divz"),
            tag("bad_toint"),
        )),
        blank,
    )(input)
    {
        Ok((remaining, code_str)) => {
            let code = match code_str {
                "stk_ovf" => TrapCode::STACK_OVERFLOW,
                "heap_oob" => TrapCode::HEAP_OUT_OF_BOUNDS,
                "int_ovf" => TrapCode::INTEGER_OVERFLOW,
                "int_divz" => TrapCode::INTEGER_DIVISION_BY_ZERO,
                "bad_toint" => TrapCode::BAD_CONVERSION_TO_INTEGER,
                _ => unreachable!(),
            };
            (remaining, code)
        }
        Err(_) => {
            // Try user-defined trap code: "user42"
            let (input, _) = terminated(tag("user"), blank)(input)?;
            let (input, num) = terminated(integer, blank)(input)?;
            let code = TrapCode::unwrap_user(num as u8);
            (input, code)
        }
    };

    Ok((input, code))
}

/// Parse a comparison instruction (icmp with condition code)
pub(crate) fn parse_comparison(input: &str) -> IResult<&str, InstData> {
    let (input, result) = terminated(parse_value, blank)(input)?;
    let (input, _) = terminated(tag("="), blank)(input)?;
    let (input, (_, cond, arg1, _, arg2)) = tuple((
        terminated(tag("icmp"), blank),
        parse_int_cond_code,
        terminated(parse_value, blank),
        terminated(char(','), blank),
        terminated(parse_value, blank),
    ))(input)?;

    Ok((
        input,
        InstData::comparison(Opcode::Icmp { cond }, result, arg1, arg2),
    ))
}

/// Parse a floating point comparison instruction (fcmp with condition code)
pub(crate) fn parse_fcmp(input: &str) -> IResult<&str, InstData> {
    let (input, result) = terminated(parse_value, blank)(input)?;
    let (input, _) = terminated(tag("="), blank)(input)?;
    let (input, _) = terminated(tag("fcmp"), blank)(input)?;
    let (input, cond) = parse_float_cond_code(input)?;
    let (input, arg1) = terminated(parse_value, blank)(input)?;
    let (input, _) = terminated(char(','), blank)(input)?;
    let (input, arg2) = terminated(parse_value, blank)(input)?;

    Ok((
        input,
        InstData::comparison(Opcode::Fcmp { cond }, result, arg1, arg2),
    ))
}

/// Parse a constant instruction (iconst, fconst)
pub(crate) fn parse_const(input: &str) -> IResult<&str, InstData> {
    let (input, result) = terminated(parse_value, blank)(input)?;
    let (input, _) = terminated(tag("="), blank)(input)?;
    let (input, op) = terminated(alt((tag("iconst"), tag("fconst"))), blank)(input)?;

    match op {
        "iconst" => {
            let (input, value) = terminated(integer, blank)(input)?;
            Ok((input, InstData::constant(result, Immediate::I64(value))))
        }
        "fconst" => {
            let (input, value) = terminated(float, blank)(input)?;
            let value_bits = value.to_bits();
            Ok((
                input,
                InstData::constant(result, Immediate::F32Bits(value_bits)),
            ))
        }
        _ => unreachable!(),
    }
}

/// Parse a stack allocation instruction (stackalloc)
pub(crate) fn parse_stackalloc(input: &str) -> IResult<&str, InstData> {
    let (input, result) = terminated(parse_value, blank)(input)?;
    let (input, _) = terminated(tag("="), blank)(input)?;
    let (input, _) = terminated(tag("stackalloc"), blank)(input)?;
    let (input, size) = terminated(integer, blank)(input)?;

    // Size must be non-negative and fit in u32
    if size < 0 {
        return Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Verify,
        )));
    }
    let size_u32 = size as u32;

    Ok((input, InstData::stackalloc(result, size_u32)))
}

/// Parse a jump instruction
pub(crate) fn parse_jump(input: &str) -> IResult<&str, InstData> {
    let (input, _) = terminated(tag("jump"), blank)(input)?;
    let (input, target) = terminated(parse_block_index, blank)(input)?;
    // Parse optional args in parentheses: (v1, v2, ...)
    let (input, args) = opt(delimited(
        terminated(char('('), blank),
        separated_list0(terminated(char(','), blank), terminated(parse_value, blank)),
        terminated(char(')'), blank),
    ))(input)?;
    let args = args.unwrap_or_default();
    let target_block = Block::from_index(target as usize);
    Ok((input, InstData::jump(target_block, args)))
}

/// Parse a branch instruction (brif)
pub(crate) fn parse_branch(input: &str) -> IResult<&str, InstData> {
    let (input, _) = terminated(tag("brif"), blank)(input)?;
    let (input, condition) = terminated(parse_value, blank)(input)?;
    let (input, _) = terminated(char(','), blank)(input)?;
    let (input, target_true) = terminated(parse_block_index, blank)(input)?;
    // Parse optional args for true target: (v1, v2, ...)
    let (input, args_true) = opt(delimited(
        terminated(char('('), blank),
        separated_list0(terminated(char(','), blank), terminated(parse_value, blank)),
        terminated(char(')'), blank),
    ))(input)?;
    let (input, _) = terminated(char(','), blank)(input)?;
    let (input, target_false) = terminated(parse_block_index, blank)(input)?;
    // Parse optional args for false target: (v1, v2, ...)
    let (input, args_false) = opt(delimited(
        terminated(char('('), blank),
        separated_list0(terminated(char(','), blank), terminated(parse_value, blank)),
        terminated(char(')'), blank),
    ))(input)?;
    let args_true = args_true.unwrap_or_default();
    let args_false = args_false.unwrap_or_default();
    let target_true_block = Block::from_index(target_true as usize);
    let target_false_block = Block::from_index(target_false as usize);
    Ok((
        input,
        InstData::branch(
            condition,
            target_true_block,
            args_true,
            target_false_block,
            args_false,
        ),
    ))
}

/// Parse a call instruction
pub(crate) fn parse_call(input: &str) -> IResult<&str, InstData> {
    let (input, _) = terminated(tag("call"), blank)(input)?;
    let (input, callee) = terminated(parse_function_name, blank)(input)?;
    let (input, args) = delimited(
        terminated(char('('), blank),
        separated_list0(terminated(char(','), blank), terminated(parse_value, blank)),
        terminated(char(')'), blank),
    )(input)?;

    // Parse results list: comma-separated values (same format as args)
    let (input, results) = opt(map(
        tuple((
            terminated(tag("->"), blank),
            separated_list0(terminated(char(','), blank), terminated(parse_value, blank)),
        )),
        |(_, values)| values,
    ))(input)?;

    Ok((
        input,
        InstData::call(callee, args, results.unwrap_or_default()),
    ))
}

/// Parse a syscall instruction
pub(crate) fn parse_syscall(input: &str) -> IResult<&str, InstData> {
    let (input, _) = terminated(tag("syscall"), blank)(input)?;
    let (input, number) = terminated(integer, blank)(input)?;
    let (input, args) = delimited(
        terminated(char('('), blank),
        separated_list0(terminated(char(','), blank), terminated(parse_value, blank)),
        terminated(char(')'), blank),
    )(input)?;

    // Parse results list: comma-separated values (same format as call)
    let (input, results) = opt(map(
        tuple((
            terminated(tag("->"), blank),
            separated_list0(terminated(char(','), blank), terminated(parse_value, blank)),
        )),
        |(_, values)| values,
    ))(input)?;

    Ok((
        input,
        InstData::syscall(number as i32, args, results.unwrap_or_default()),
    ))
}

/// Parse a load instruction
pub(crate) fn parse_load(input: &str) -> IResult<&str, InstData> {
    let (input, result) = terminated(parse_value, blank)(input)?;
    let (input, _) = terminated(tag("="), blank)(input)?;
    // Check if it's "load" using peek to avoid consuming input if it's not
    let (input, _) = preceded(peek(tag("load")), tag("load"))(input)?;
    let (input, _) = char('.')(input)?;
    let (input, ty) = terminated(parse_type, blank)(input)?;
    let (input, address) = terminated(parse_value, blank)(input)?;
    Ok((input, InstData::load(result, address, ty)))
}

/// Parse a store instruction
pub(crate) fn parse_store(input: &str) -> IResult<&str, InstData> {
    let (input, _) = terminated(tag("store"), char('.'))(input)?;
    let (input, ty) = terminated(parse_type, blank)(input)?;
    let (input, address) = terminated(parse_value, blank)(input)?;
    let (input, _) = terminated(char(','), blank)(input)?;
    let (input, value) = terminated(parse_value, blank)(input)?;
    Ok((input, InstData::store(address, value, ty)))
}

/// Parse a return instruction
pub(crate) fn parse_return(input: &str) -> IResult<&str, InstData> {
    let (input, _) = terminated(tag("return"), blank)(input)?;
    // Parse comma-separated values (CLIF format)
    let (input, values) =
        separated_list0(terminated(char(','), blank), terminated(parse_value, blank))(input)?;
    Ok((input, InstData::return_(values)))
}

/// Parse a halt instruction
pub(crate) fn parse_halt(input: &str) -> IResult<&str, InstData> {
    map(terminated(tag("halt"), blank), |_| InstData::halt())(input)
}

/// Parse a trap instruction
pub(crate) fn parse_trap(input: &str) -> IResult<&str, InstData> {
    let (input, _) = terminated(tag("trap"), blank)(input)?;
    let (input, code) = parse_trap_code(input)?;
    Ok((input, InstData::trap(code)))
}

/// Parse a trapz instruction (trap if condition is zero)
pub(crate) fn parse_trapz(input: &str) -> IResult<&str, InstData> {
    let (input, _) = terminated(tag("trapz"), blank)(input)?;
    let (input, condition) = terminated(parse_value, blank)(input)?;
    let (input, _) = terminated(char(','), blank)(input)?;
    let (input, code) = parse_trap_code(input)?;
    Ok((input, InstData::trapz(condition, code)))
}

/// Parse a trapnz instruction (trap if condition is non-zero)
pub(crate) fn parse_trapnz(input: &str) -> IResult<&str, InstData> {
    let (input, _) = terminated(tag("trapnz"), blank)(input)?;
    let (input, condition) = terminated(parse_value, blank)(input)?;
    let (input, _) = terminated(char(','), blank)(input)?;
    let (input, code) = parse_trap_code(input)?;
    Ok((input, InstData::trapnz(condition, code)))
}

/// Parse any instruction
pub(crate) fn parse_instruction(input: &str) -> IResult<&str, InstData> {
    // Order matters: more specific patterns first
    // Instructions that don't start with "v0 = " come first
    alt((
        parse_store,   // "store.i32 v0, v1" - doesn't start with value assignment
        parse_call,    // "call %func(v0)" - doesn't start with value assignment
        parse_syscall, // "syscall 1(v0)" - doesn't start with value assignment
        parse_branch,  // "brif v0, block1, block2" - doesn't start with value assignment
        parse_jump,    // "jump block1" - doesn't start with value assignment
        parse_return,  // "return v0" - doesn't start with value assignment
        parse_halt,    // "halt" - doesn't start with value assignment
        parse_trap,    // "trap int_divz" - doesn't start with value assignment
        parse_trapz,   // "trapz v0, int_divz" - doesn't start with value assignment
        parse_trapnz,  // "trapnz v0, int_ovf" - doesn't start with value assignment
        // Instructions that start with "v0 = " come after
        // Try parse_const before parse_stackalloc before parse_load since "iconst" and "stackalloc" are more specific than "load"
        parse_const,            // "v0 = iconst 42" or "v0 = fconst 3.14"
        parse_stackalloc,       // "v0 = stackalloc 4" - stack allocation
        parse_load,             // "v0 = load.i32 v1" - has type suffix
        parse_fcmp,             // "v0 = fcmp eq v1, v2" - must come before parse_comparison
        parse_comparison,       // "v0 = icmp eq v1, v2" or "v0 = icmp_eq v1, v2"
        parse_float_arithmetic, // "v0 = fadd v1, v2" - must come before parse_arithmetic
        parse_bitwise_unary,    // "v0 = inot v1" - unary, must come before binary bitwise
        parse_bitwise,          // "v0 = iand v1, v2" or "v0 = ior v1, v2" or "v0 = ixor v1, v2"
        parse_shift,            // "v0 = ishl v1, v2" or "v0 = ishr v1, v2" or "v0 = iashr v1, v2"
        parse_arithmetic,       // "v0 = iadd v1, v2"
    ))(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_const_iconst() {
        let input = "v0 = iconst 42";
        let result = parse_const(input);
        assert!(result.is_ok(), "parse_const failed: {:?}", result);
        let (remaining, inst_data) = result.unwrap();
        assert_eq!(
            remaining, "",
            "Should consume all input, got: {:?}",
            remaining
        );
        assert_eq!(inst_data.opcode, Opcode::Iconst);
        if let Some(Immediate::I64(value)) = inst_data.imm {
            assert_eq!(value, 42);
        } else {
            panic!("Expected I64 immediate, got: {:?}", inst_data.imm);
        }
    }

    #[test]
    fn test_parse_const_fconst() {
        let input = "v0 = fconst 3.14";
        let result = parse_const(input);
        assert!(result.is_ok(), "parse_const failed: {:?}", result);
        let (remaining, inst_data) = result.unwrap();
        assert_eq!(remaining, "", "Should consume all input");
        assert_eq!(inst_data.opcode, Opcode::Fconst);
        // Verify it's f32 (value_bits is u32)
        if let Some(Immediate::F32Bits(value_bits)) = inst_data.imm {
            let f32_value = f32::from_bits(value_bits);
            assert!((f32_value - 3.14f32).abs() < 0.01);
        } else {
            panic!("Expected F32Bits immediate, got: {:?}", inst_data.imm);
        }
    }

    #[test]
    fn test_parse_instruction_iconst() {
        let input = "v0 = iconst 42";
        let result = parse_instruction(input);
        assert!(result.is_ok(), "parse_instruction failed: {:?}", result);
        let (remaining, inst_data) = result.unwrap();
        assert_eq!(remaining, "", "Should consume all input");
        assert_eq!(inst_data.opcode, Opcode::Iconst);
    }

    #[test]
    fn test_parse_arithmetic() {
        let input = "v0 = iadd v1, v2";
        let result = parse_instruction(input);
        assert!(result.is_ok(), "parse_instruction failed: {:?}", result);
        let (_, inst_data) = result.unwrap();
        assert_eq!(inst_data.opcode, Opcode::Iadd);
    }

    #[test]
    fn test_parse_bitwise() {
        let input = "v0 = iand v1, v2";
        let result = parse_bitwise(input);
        assert!(result.is_ok(), "parse_bitwise failed: {:?}", result);
        let (_, inst_data) = result.unwrap();
        assert_eq!(inst_data.opcode, Opcode::Iand);

        let input = "v0 = ior v1, v2";
        let result = parse_bitwise(input);
        assert!(result.is_ok(), "parse_bitwise failed: {:?}", result);
        let (_, inst_data) = result.unwrap();
        assert_eq!(inst_data.opcode, Opcode::Ior);

        let input = "v0 = ixor v1, v2";
        let result = parse_bitwise(input);
        assert!(result.is_ok(), "parse_bitwise failed: {:?}", result);
        let (_, inst_data) = result.unwrap();
        assert_eq!(inst_data.opcode, Opcode::Ixor);
    }

    #[test]
    fn test_parse_bitwise_unary() {
        let input = "v0 = inot v1";
        let result = parse_bitwise_unary(input);
        assert!(result.is_ok(), "parse_bitwise_unary failed: {:?}", result);
        let (_, inst_data) = result.unwrap();
        assert_eq!(inst_data.opcode, Opcode::Inot);
    }

    #[test]
    fn test_parse_shift() {
        let input = "v0 = ishl v1, v2";
        let result = parse_shift(input);
        assert!(result.is_ok(), "parse_shift failed: {:?}", result);
        let (_, inst_data) = result.unwrap();
        assert_eq!(inst_data.opcode, Opcode::Ishl);

        let input = "v0 = ishr v1, v2";
        let result = parse_shift(input);
        assert!(result.is_ok(), "parse_shift failed: {:?}", result);
        let (_, inst_data) = result.unwrap();
        assert_eq!(inst_data.opcode, Opcode::Ishr);

        let input = "v0 = iashr v1, v2";
        let result = parse_shift(input);
        assert!(result.is_ok(), "parse_shift failed: {:?}", result);
        let (_, inst_data) = result.unwrap();
        assert_eq!(inst_data.opcode, Opcode::Iashr);
    }

    #[test]
    fn test_parse_instruction_bitwise() {
        let input = "v0 = iand v1, v2";
        let result = parse_instruction(input);
        assert!(result.is_ok(), "parse_instruction failed: {:?}", result);
        let (_, inst_data) = result.unwrap();
        assert_eq!(inst_data.opcode, Opcode::Iand);

        let input = "v0 = inot v1";
        let result = parse_instruction(input);
        assert!(result.is_ok(), "parse_instruction failed: {:?}", result);
        let (_, inst_data) = result.unwrap();
        assert_eq!(inst_data.opcode, Opcode::Inot);

        let input = "v0 = ishl v1, v2";
        let result = parse_instruction(input);
        assert!(result.is_ok(), "parse_instruction failed: {:?}", result);
        let (_, inst_data) = result.unwrap();
        assert_eq!(inst_data.opcode, Opcode::Ishl);
    }

    #[test]
    fn test_parse_return() {
        let input = "return";
        let result = parse_return(input);
        assert!(result.is_ok());
        let (_, inst_data) = result.unwrap();
        assert_eq!(inst_data.opcode, Opcode::Return);
        assert_eq!(inst_data.args.len(), 0);
    }

    #[test]
    fn test_parse_return_with_values() {
        let input = "return v0, v1";
        let result = parse_return(input);
        assert!(result.is_ok());
        let (_, inst_data) = result.unwrap();
        assert_eq!(inst_data.opcode, Opcode::Return);
        assert_eq!(inst_data.args.len(), 2);
    }

    #[test]
    fn test_parse_comparison() {
        let input = "v0 = icmp eq v1, v2";
        let result = parse_comparison(input);
        assert!(result.is_ok(), "parse_comparison failed: {:?}", result);
        let (_, inst_data) = result.unwrap();
        match &inst_data.opcode {
            Opcode::Icmp { cond } => assert_eq!(*cond, crate::condcodes::IntCC::Equal),
            _ => panic!("Expected Icmp opcode"),
        }
    }

    #[test]
    fn test_parse_comparison_rejects_old_format() {
        // Old format should be rejected
        let input = "v0 = icmp_eq v1, v2";
        let result = parse_comparison(input);
        assert!(result.is_err(), "parse_comparison should reject old format");
    }

    #[test]
    fn test_parse_jump() {
        let input = "jump block1";
        let result = parse_jump(input);
        assert!(result.is_ok(), "parse_jump failed: {:?}", result);
        let (_, inst_data) = result.unwrap();
        assert_eq!(inst_data.opcode, Opcode::Jump);
        assert!(inst_data.block_args.is_some());
        let block_args = inst_data.block_args.as_ref().unwrap();
        assert_eq!(block_args.targets.len(), 1);
        assert_eq!(block_args.targets[0].0.index(), 1);
        assert_eq!(block_args.targets[0].1.len(), 0);
    }

    #[test]
    fn test_parse_jump_with_args() {
        let input = "jump block3(v1, v2)";
        let result = parse_jump(input);
        assert!(result.is_ok(), "parse_jump failed: {:?}", result);
        let (_, inst_data) = result.unwrap();
        assert_eq!(inst_data.opcode, Opcode::Jump);
        assert!(inst_data.block_args.is_some());
        let block_args = inst_data.block_args.as_ref().unwrap();
        assert_eq!(block_args.targets.len(), 1);
        assert_eq!(block_args.targets[0].0.index(), 3);
        assert_eq!(block_args.targets[0].1.len(), 2);
        assert_eq!(block_args.targets[0].1[0].index(), 1);
        assert_eq!(block_args.targets[0].1[1].index(), 2);
    }

    #[test]
    fn test_parse_branch() {
        let input = "brif v0, block1, block2";
        let result = parse_branch(input);
        assert!(result.is_ok(), "parse_branch failed: {:?}", result);
        let (_, inst_data) = result.unwrap();
        assert_eq!(inst_data.opcode, Opcode::Br);
        assert!(inst_data.block_args.is_some());
        let block_args = inst_data.block_args.as_ref().unwrap();
        assert_eq!(block_args.targets.len(), 2);
        assert_eq!(block_args.targets[0].0.index(), 1);
        assert_eq!(block_args.targets[0].1.len(), 0);
        assert_eq!(block_args.targets[1].0.index(), 2);
        assert_eq!(block_args.targets[1].1.len(), 0);
    }

    #[test]
    fn test_parse_branch_with_args() {
        let input = "brif v0, block1(v1), block2(v2)";
        let result = parse_branch(input);
        assert!(result.is_ok(), "parse_branch failed: {:?}", result);
        let (_, inst_data) = result.unwrap();
        assert_eq!(inst_data.opcode, Opcode::Br);
        assert_eq!(inst_data.args[0].index(), 0);
        assert!(inst_data.block_args.is_some());
        let block_args = inst_data.block_args.as_ref().unwrap();
        assert_eq!(block_args.targets.len(), 2);
        assert_eq!(block_args.targets[0].0.index(), 1);
        assert_eq!(block_args.targets[0].1.len(), 1);
        assert_eq!(block_args.targets[0].1[0].index(), 1);
        assert_eq!(block_args.targets[1].0.index(), 2);
        assert_eq!(block_args.targets[1].1.len(), 1);
        assert_eq!(block_args.targets[1].1[0].index(), 2);
    }

    #[test]
    fn test_parse_branch_mixed_args() {
        // CLIF format: one target with args, one without (parentheses optional)
        let input = "brif v0, block1, block2(v1)";
        let result = parse_branch(input);
        assert!(result.is_ok(), "parse_branch failed: {:?}", result);
        let (_, inst_data) = result.unwrap();
        assert_eq!(inst_data.opcode, Opcode::Br);
        assert!(inst_data.block_args.is_some());
        let block_args = inst_data.block_args.as_ref().unwrap();
        assert_eq!(block_args.targets[0].0.index(), 1);
        assert_eq!(
            block_args.targets[0].1.len(),
            0,
            "block1 should have no args"
        );
        assert_eq!(block_args.targets[1].0.index(), 2);
        assert_eq!(block_args.targets[1].1.len(), 1);
        assert_eq!(block_args.targets[1].1[0].index(), 1);
    }

    #[test]
    fn test_parse_branch_mixed_args_reverse() {
        // CLIF format: one target with args, one without (reversed)
        let input = "brif v0, block1(v1), block2";
        let result = parse_branch(input);
        assert!(result.is_ok(), "parse_branch failed: {:?}", result);
        let (_, inst_data) = result.unwrap();
        assert_eq!(inst_data.opcode, Opcode::Br);
        assert!(inst_data.block_args.is_some());
        let block_args = inst_data.block_args.as_ref().unwrap();
        assert_eq!(block_args.targets[0].0.index(), 1);
        assert_eq!(block_args.targets[0].1.len(), 1);
        assert_eq!(block_args.targets[0].1[0].index(), 1);
        assert_eq!(block_args.targets[1].0.index(), 2);
        assert_eq!(
            block_args.targets[1].1.len(),
            0,
            "block2 should have no args"
        );
    }

    #[test]
    fn test_parse_call() {
        let input = "call %func(v0, v1) -> v2";
        let result = parse_call(input);
        assert!(result.is_ok(), "parse_call failed: {:?}", result);
        let (remaining, inst_data) = result.unwrap();
        assert_eq!(
            remaining, "",
            "Should consume all input, got: {:?}",
            remaining
        );
        match &inst_data.opcode {
            Opcode::Call { callee } => {
                assert_eq!(callee, "func");
            }
            _ => panic!("Expected Call opcode"),
        }
        assert_eq!(inst_data.args.len(), 2);
        assert_eq!(
            inst_data.results.len(),
            1,
            "Expected 1 result, got: {:?}",
            inst_data.results
        );
    }

    #[test]
    fn test_parse_call_no_results() {
        let input = "call %func(v0)";
        let result = parse_call(input);
        assert!(result.is_ok(), "parse_call failed: {:?}", result);
        let (_, inst_data) = result.unwrap();
        assert_eq!(inst_data.results.len(), 0);
    }

    #[test]
    fn test_parse_syscall() {
        let input = "syscall 1(v0, v1)";
        let result = parse_syscall(input);
        assert!(result.is_ok(), "parse_syscall failed: {:?}", result);
        let (_, inst_data) = result.unwrap();
        assert_eq!(inst_data.opcode, Opcode::Syscall);
        if let Some(Immediate::I32(number)) = inst_data.imm {
            assert_eq!(number, 1);
        } else {
            panic!("Expected I32 immediate");
        }
        assert_eq!(inst_data.args.len(), 2);
        assert_eq!(
            inst_data.results.len(),
            0,
            "Syscall without return value should have empty results"
        );
    }

    #[test]
    fn test_parse_syscall_with_return() {
        let input = "syscall 1(v0, v1) -> v2";
        let result = parse_syscall(input);
        assert!(result.is_ok(), "parse_syscall failed: {:?}", result);
        let (_, inst_data) = result.unwrap();
        assert_eq!(inst_data.opcode, Opcode::Syscall);
        if let Some(Immediate::I32(number)) = inst_data.imm {
            assert_eq!(number, 1);
        } else {
            panic!("Expected I32 immediate");
        }
        assert_eq!(inst_data.args.len(), 2);
        assert_eq!(
            inst_data.results.len(),
            1,
            "Syscall with return value should have one result"
        );
        assert_eq!(inst_data.results[0].index(), 2);
    }

    #[test]
    fn test_parse_load() {
        let input = "v0 = load.i32 v1";
        let result = parse_load(input);
        assert!(result.is_ok(), "parse_load failed: {:?}", result);
        let (_, inst_data) = result.unwrap();
        assert_eq!(inst_data.opcode, Opcode::Load);
        assert_eq!(inst_data.ty, Some(crate::Type::I32));
    }

    #[test]
    fn test_parse_store() {
        let input = "store.i32 v0, v1";
        let result = parse_store(input);
        assert!(result.is_ok(), "parse_store failed: {:?}", result);
        let (_, inst_data) = result.unwrap();
        assert_eq!(inst_data.opcode, Opcode::Store);
        assert_eq!(inst_data.ty, Some(crate::Type::I32));
    }

    #[test]
    fn test_parse_halt() {
        let input = "halt";
        let result = parse_halt(input);
        assert!(result.is_ok(), "parse_halt failed: {:?}", result);
        let (_, inst_data) = result.unwrap();
        assert_eq!(inst_data.opcode, Opcode::Halt);
    }

    #[test]
    fn test_parse_instruction_invalid() {
        // Test that invalid instruction fails
        let result = parse_instruction("invalid");
        assert!(result.is_err(), "Should fail on invalid instruction");
    }

    #[test]
    fn test_parse_instruction_empty() {
        // Test that empty input fails
        let result = parse_instruction("");
        assert!(result.is_err(), "Should fail on empty input");
    }

    #[test]
    fn test_parse_arithmetic_missing_args() {
        // Test that missing arguments fail
        let result = parse_instruction("v0 = iadd");
        assert!(result.is_err(), "Should fail on missing arguments");
    }

    #[test]
    fn test_parse_const_missing_value() {
        // Test that missing constant value fails
        let result = parse_instruction("v0 = iconst");
        assert!(result.is_err(), "Should fail on missing constant value");
    }

    #[test]
    fn test_parse_call_missing_parens() {
        // Test that missing parentheses fail
        let result = parse_instruction("call %func");
        assert!(result.is_err(), "Should fail on missing parentheses");
    }

    #[test]
    fn test_parse_call_with_whitespace_after() {
        // Test that call instruction consumes trailing whitespace correctly
        let input = "call %helper(v10) -> v11\n        v12 = iconst 100";
        let result = parse_call(input);
        assert!(result.is_ok(), "parse_call failed: {:?}", result);
        let (remaining, inst_data) = result.unwrap();
        // Should consume the call and whitespace, leaving the next instruction
        assert!(
            remaining.trim_start().starts_with("v12"),
            "Should leave next instruction, got: {:?}",
            remaining
        );
        match &inst_data.opcode {
            Opcode::Call { callee } => {
                assert_eq!(callee, "helper");
            }
            _ => panic!("Expected Call opcode"),
        }
        assert_eq!(inst_data.args.len(), 1);
        assert_eq!(inst_data.results.len(), 1);
    }

    #[test]
    fn test_parse_call_followed_by_instruction() {
        // Test parsing call followed by another instruction
        let input = "call %helper(v10) -> v11\n        v12 = iconst 100";
        let result = parse_instruction(input);
        assert!(result.is_ok(), "parse_instruction failed: {:?}", result);
        let (remaining, inst_data) = result.unwrap();
        assert!(
            remaining.trim_start().starts_with("v12"),
            "Should leave next instruction, got: {:?}",
            remaining
        );
        match inst_data.opcode {
            Opcode::Call { .. } => {}
            _ => panic!("Expected Call opcode"),
        }
    }

    #[test]
    fn test_parse_call_multiple_returns() {
        // Test call with multiple return values
        let input = "call %func(v0, v1) -> v2, v3, v4";
        let result = parse_call(input);
        assert!(result.is_ok(), "parse_call failed: {:?}", result);
        let (remaining, inst_data) = result.unwrap();
        assert_eq!(
            remaining, "",
            "Should consume all input, got: {:?}",
            remaining
        );
        match &inst_data.opcode {
            Opcode::Call { callee } => {
                assert_eq!(callee, "func");
            }
            _ => panic!("Expected Call opcode"),
        }
        assert_eq!(inst_data.args.len(), 2);
        assert_eq!(
            inst_data.results.len(),
            3,
            "Expected 3 results, got: {:?}",
            inst_data.results
        );
    }

    #[test]
    fn test_parse_call_multiple_returns_via_instruction() {
        // Test parsing call with multiple returns via parse_instruction
        let input = "call %helper(v10) -> v11, v12, v13";
        let result = parse_instruction(input);
        assert!(result.is_ok(), "parse_instruction failed: {:?}", result);
        let (remaining, inst_data) = result.unwrap();
        assert_eq!(remaining, "", "Should consume all input");
        match inst_data.opcode {
            Opcode::Call { .. } => {
                assert_eq!(
                    inst_data.results.len(),
                    3,
                    "Expected 3 results, got: {:?}",
                    inst_data.results
                );
            }
            _ => panic!("Expected Call opcode"),
        }
    }

    #[test]
    fn test_parse_return_multiple_values() {
        // Test return with multiple values (more than 2)
        let input = "return v0, v1, v2, v3";
        let result = parse_return(input);
        assert!(result.is_ok());
        let (_, inst_data) = result.unwrap();
        assert_eq!(inst_data.opcode, Opcode::Return);
        assert_eq!(
            inst_data.args.len(),
            4,
            "Expected 4 values, got: {:?}",
            inst_data.args
        );
    }

    #[test]
    fn test_parse_return_multiple_values_via_instruction() {
        // Test parsing return with multiple values via parse_instruction
        let input = "return v0, v1, v2, v3, v4";
        let result = parse_instruction(input);
        assert!(result.is_ok(), "parse_instruction failed: {:?}", result);
        let (remaining, inst_data) = result.unwrap();
        assert_eq!(remaining, "", "Should consume all input");
        assert_eq!(inst_data.opcode, Opcode::Return);
        assert_eq!(
            inst_data.args.len(),
            5,
            "Expected 5 values, got: {:?}",
            inst_data.args
        );
    }

    #[test]
    fn test_parse_return_empty() {
        // Test that return without values is valid
        let result = parse_instruction("return");
        assert!(result.is_ok(), "Return without values should be valid");
    }
}
