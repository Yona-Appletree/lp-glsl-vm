//! Instruction parsers.

use nom::{
    branch::alt,
    bytes::complete::tag,
    character::complete::char,
    combinator::{map, opt, peek},
    multi::{many0, separated_list0},
    sequence::{delimited, preceded, terminated, tuple},
    IResult,
};

use super::{
    primitives::{float, integer, parse_block_index, parse_function_name, parse_type, parse_value},
    whitespace::blank,
};
use crate::inst::Inst;

/// Parse an arithmetic instruction (iadd, isub, imul, idiv, irem)
pub(crate) fn parse_arithmetic(input: &str) -> IResult<&str, Inst> {
    let (input, result) = terminated(parse_value, blank)(input)?;
    let (input, _) = terminated(tag("="), blank)(input)?;
    let (input, op) = terminated(
        alt((
            tag("iadd"),
            tag("isub"),
            tag("imul"),
            tag("idiv"),
            tag("irem"),
        )),
        blank,
    )(input)?;
    let (input, arg1) = terminated(parse_value, blank)(input)?;
    let (input, _) = terminated(char(','), blank)(input)?;
    let (input, arg2) = terminated(parse_value, blank)(input)?;

    let inst = match op {
        "iadd" => Inst::Iadd { result, arg1, arg2 },
        "isub" => Inst::Isub { result, arg1, arg2 },
        "imul" => Inst::Imul { result, arg1, arg2 },
        "idiv" => Inst::Idiv { result, arg1, arg2 },
        "irem" => Inst::Irem { result, arg1, arg2 },
        _ => unreachable!(),
    };

    Ok((input, inst))
}

/// Parse a comparison instruction (icmp_eq, icmp_ne, etc.)
pub(crate) fn parse_comparison(input: &str) -> IResult<&str, Inst> {
    let (input, result) = terminated(parse_value, blank)(input)?;
    let (input, _) = terminated(tag("="), blank)(input)?;
    let (input, op) = terminated(
        alt((
            tag("icmp_eq"),
            tag("icmp_ne"),
            tag("icmp_lt"),
            tag("icmp_le"),
            tag("icmp_gt"),
            tag("icmp_ge"),
        )),
        blank,
    )(input)?;
    let (input, arg1) = terminated(parse_value, blank)(input)?;
    let (input, _) = terminated(char(','), blank)(input)?;
    let (input, arg2) = terminated(parse_value, blank)(input)?;

    let inst = match op {
        "icmp_eq" => Inst::IcmpEq { result, arg1, arg2 },
        "icmp_ne" => Inst::IcmpNe { result, arg1, arg2 },
        "icmp_lt" => Inst::IcmpLt { result, arg1, arg2 },
        "icmp_le" => Inst::IcmpLe { result, arg1, arg2 },
        "icmp_gt" => Inst::IcmpGt { result, arg1, arg2 },
        "icmp_ge" => Inst::IcmpGe { result, arg1, arg2 },
        _ => unreachable!(),
    };

    Ok((input, inst))
}

/// Parse a constant instruction (iconst, fconst)
pub(crate) fn parse_const(input: &str) -> IResult<&str, Inst> {
    let (input, result) = terminated(parse_value, blank)(input)?;
    let (input, _) = terminated(tag("="), blank)(input)?;
    let (input, op) = terminated(alt((tag("iconst"), tag("fconst"))), blank)(input)?;

    match op {
        "iconst" => {
            let (input, value) = terminated(integer, blank)(input)?;
            Ok((input, Inst::Iconst { result, value }))
        }
        "fconst" => {
            let (input, value) = terminated(float, blank)(input)?;
            let value_bits = value.to_bits();
            Ok((input, Inst::Fconst { result, value_bits }))
        }
        _ => unreachable!(),
    }
}

/// Parse a jump instruction
pub(crate) fn parse_jump(input: &str) -> IResult<&str, Inst> {
    let (input, _) = terminated(tag("jump"), blank)(input)?;
    let (input, target) = terminated(parse_block_index, blank)(input)?;
    // Parse optional args in parentheses: (v1, v2, ...)
    let (input, args) = opt(delimited(
        terminated(char('('), blank),
        separated_list0(terminated(char(','), blank), terminated(parse_value, blank)),
        terminated(char(')'), blank),
    ))(input)?;
    Ok((
        input,
        Inst::Jump {
            target,
            args: args.unwrap_or_default(),
        },
    ))
}

/// Parse a branch instruction (brif)
pub(crate) fn parse_branch(input: &str) -> IResult<&str, Inst> {
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
    Ok((
        input,
        Inst::Br {
            condition,
            target_true,
            args_true: args_true.unwrap_or_default(),
            target_false,
            args_false: args_false.unwrap_or_default(),
        },
    ))
}

/// Parse a call instruction
pub(crate) fn parse_call(input: &str) -> IResult<&str, Inst> {
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
        Inst::Call {
            callee,
            args,
            results: results.unwrap_or_default(),
        },
    ))
}

/// Parse a syscall instruction
pub(crate) fn parse_syscall(input: &str) -> IResult<&str, Inst> {
    let (input, _) = terminated(tag("syscall"), blank)(input)?;
    let (input, number) = terminated(integer, blank)(input)?;
    let (input, args) = delimited(
        terminated(char('('), blank),
        separated_list0(terminated(char(','), blank), terminated(parse_value, blank)),
        terminated(char(')'), blank),
    )(input)?;
    Ok((
        input,
        Inst::Syscall {
            number: number as i32,
            args,
        },
    ))
}

/// Parse a load instruction
pub(crate) fn parse_load(input: &str) -> IResult<&str, Inst> {
    let (input, result) = terminated(parse_value, blank)(input)?;
    let (input, _) = terminated(tag("="), blank)(input)?;
    // Check if it's "load" using peek to avoid consuming input if it's not
    let (input, _) = preceded(peek(tag("load")), tag("load"))(input)?;
    let (input, _) = char('.')(input)?;
    let (input, ty) = terminated(parse_type, blank)(input)?;
    let (input, address) = terminated(parse_value, blank)(input)?;
    Ok((
        input,
        Inst::Load {
            result,
            address,
            ty,
        },
    ))
}

/// Parse a store instruction
pub(crate) fn parse_store(input: &str) -> IResult<&str, Inst> {
    let (input, _) = terminated(tag("store"), char('.'))(input)?;
    let (input, ty) = terminated(parse_type, blank)(input)?;
    let (input, address) = terminated(parse_value, blank)(input)?;
    let (input, _) = terminated(char(','), blank)(input)?;
    let (input, value) = terminated(parse_value, blank)(input)?;
    Ok((input, Inst::Store { address, value, ty }))
}

/// Parse a return instruction
pub(crate) fn parse_return(input: &str) -> IResult<&str, Inst> {
    let (input, _) = terminated(tag("return"), blank)(input)?;
    // Parse space-separated values (Cranelift format)
    // Values are separated by spaces, not commas
    let (input, values) = many0(terminated(parse_value, blank))(input)?;
    Ok((input, Inst::Return { values }))
}

/// Parse a halt instruction
pub(crate) fn parse_halt(input: &str) -> IResult<&str, Inst> {
    map(terminated(tag("halt"), blank), |_| Inst::Halt)(input)
}

/// Parse any instruction
pub(crate) fn parse_instruction(input: &str) -> IResult<&str, Inst> {
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
        // Instructions that start with "v0 = " come after
        // Try parse_const before parse_load since "iconst" is more specific than "load"
        parse_const,      // "v0 = iconst 42" or "v0 = fconst 3.14"
        parse_load,       // "v0 = load.i32 v1" - has type suffix
        parse_comparison, // "v0 = icmp_eq v1, v2"
        parse_arithmetic, // "v0 = iadd v1, v2"
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
        let (remaining, inst) = result.unwrap();
        assert_eq!(
            remaining, "",
            "Should consume all input, got: {:?}",
            remaining
        );
        if let Inst::Iconst { value, .. } = inst {
            assert_eq!(value, 42);
        } else {
            panic!("Expected Iconst, got: {:?}", inst);
        }
    }

    #[test]
    fn test_parse_const_fconst() {
        let input = "v0 = fconst 3.14";
        let result = parse_const(input);
        assert!(result.is_ok(), "parse_const failed: {:?}", result);
        let (remaining, inst) = result.unwrap();
        assert_eq!(remaining, "", "Should consume all input");
        assert!(matches!(inst, Inst::Fconst { .. }));
        // Verify it's f32 (value_bits is u32)
        if let Inst::Fconst { value_bits, .. } = inst {
            let f32_value = f32::from_bits(value_bits);
            assert!((f32_value - 3.14f32).abs() < 0.01);
        }
    }

    #[test]
    fn test_parse_instruction_iconst() {
        let input = "v0 = iconst 42";
        let result = parse_instruction(input);
        assert!(result.is_ok(), "parse_instruction failed: {:?}", result);
        let (remaining, inst) = result.unwrap();
        assert_eq!(remaining, "", "Should consume all input");
        assert!(matches!(inst, Inst::Iconst { .. }));
    }

    #[test]
    fn test_parse_arithmetic() {
        let input = "v0 = iadd v1, v2";
        let result = parse_instruction(input);
        assert!(result.is_ok(), "parse_instruction failed: {:?}", result);
        let (_, inst) = result.unwrap();
        assert!(matches!(inst, Inst::Iadd { .. }));
    }

    #[test]
    fn test_parse_return() {
        let input = "return";
        let result = parse_return(input);
        assert!(result.is_ok());
        let (_, inst) = result.unwrap();
        if let Inst::Return { values } = inst {
            assert_eq!(values.len(), 0);
        } else {
            panic!("Expected Return");
        }
    }

    #[test]
    fn test_parse_return_with_values() {
        let input = "return v0 v1";
        let result = parse_return(input);
        assert!(result.is_ok());
        let (_, inst) = result.unwrap();
        if let Inst::Return { values } = inst {
            assert_eq!(values.len(), 2);
        } else {
            panic!("Expected Return with 2 values");
        }
    }

    #[test]
    fn test_parse_comparison() {
        let input = "v0 = icmp_eq v1, v2";
        let result = parse_comparison(input);
        assert!(result.is_ok(), "parse_comparison failed: {:?}", result);
        let (_, inst) = result.unwrap();
        assert!(matches!(inst, Inst::IcmpEq { .. }));
    }

    #[test]
    fn test_parse_jump() {
        let input = "jump block1";
        let result = parse_jump(input);
        assert!(result.is_ok(), "parse_jump failed: {:?}", result);
        let (_, inst) = result.unwrap();
        if let Inst::Jump { target, args } = inst {
            assert_eq!(target, 1);
            assert_eq!(args.len(), 0);
        } else {
            panic!("Expected Jump");
        }
    }

    #[test]
    fn test_parse_jump_with_args() {
        let input = "jump block3(v1, v2)";
        let result = parse_jump(input);
        assert!(result.is_ok(), "parse_jump failed: {:?}", result);
        let (_, inst) = result.unwrap();
        if let Inst::Jump { target, args } = inst {
            assert_eq!(target, 3);
            assert_eq!(args.len(), 2);
            assert_eq!(args[0].index(), 1);
            assert_eq!(args[1].index(), 2);
        } else {
            panic!("Expected Jump");
        }
    }

    #[test]
    fn test_parse_branch() {
        let input = "brif v0, block1, block2";
        let result = parse_branch(input);
        assert!(result.is_ok(), "parse_branch failed: {:?}", result);
        let (_, inst) = result.unwrap();
        if let Inst::Br {
            target_true,
            args_true,
            target_false,
            args_false,
            ..
        } = inst
        {
            assert_eq!(target_true, 1);
            assert_eq!(args_true.len(), 0);
            assert_eq!(target_false, 2);
            assert_eq!(args_false.len(), 0);
        } else {
            panic!("Expected Br");
        }
    }

    #[test]
    fn test_parse_branch_with_args() {
        let input = "brif v0, block1(v1), block2(v2)";
        let result = parse_branch(input);
        assert!(result.is_ok(), "parse_branch failed: {:?}", result);
        let (_, inst) = result.unwrap();
        if let Inst::Br {
            condition,
            target_true,
            args_true,
            target_false,
            args_false,
        } = inst
        {
            assert_eq!(condition.index(), 0);
            assert_eq!(target_true, 1);
            assert_eq!(args_true.len(), 1);
            assert_eq!(args_true[0].index(), 1);
            assert_eq!(target_false, 2);
            assert_eq!(args_false.len(), 1);
            assert_eq!(args_false[0].index(), 2);
        } else {
            panic!("Expected Br");
        }
    }

    #[test]
    fn test_parse_branch_mixed_args() {
        // CLIF format: one target with args, one without (parentheses optional)
        let input = "brif v0, block1, block2(v1)";
        let result = parse_branch(input);
        assert!(result.is_ok(), "parse_branch failed: {:?}", result);
        let (_, inst) = result.unwrap();
        if let Inst::Br {
            condition,
            target_true,
            args_true,
            target_false,
            args_false,
        } = inst
        {
            assert_eq!(condition.index(), 0);
            assert_eq!(target_true, 1);
            assert_eq!(args_true.len(), 0, "block1 should have no args");
            assert_eq!(target_false, 2);
            assert_eq!(args_false.len(), 1);
            assert_eq!(args_false[0].index(), 1);
        } else {
            panic!("Expected Br");
        }
    }

    #[test]
    fn test_parse_branch_mixed_args_reverse() {
        // CLIF format: one target with args, one without (reversed)
        let input = "brif v0, block1(v1), block2";
        let result = parse_branch(input);
        assert!(result.is_ok(), "parse_branch failed: {:?}", result);
        let (_, inst) = result.unwrap();
        if let Inst::Br {
            condition,
            target_true,
            args_true,
            target_false,
            args_false,
        } = inst
        {
            assert_eq!(condition.index(), 0);
            assert_eq!(target_true, 1);
            assert_eq!(args_true.len(), 1);
            assert_eq!(args_true[0].index(), 1);
            assert_eq!(target_false, 2);
            assert_eq!(args_false.len(), 0, "block2 should have no args");
        } else {
            panic!("Expected Br");
        }
    }

    #[test]
    fn test_parse_call() {
        let input = "call %func(v0, v1) -> v2";
        let result = parse_call(input);
        assert!(result.is_ok(), "parse_call failed: {:?}", result);
        let (remaining, inst) = result.unwrap();
        assert_eq!(
            remaining, "",
            "Should consume all input, got: {:?}",
            remaining
        );
        if let Inst::Call {
            callee,
            args,
            results,
        } = inst
        {
            assert_eq!(callee, "func");
            assert_eq!(args.len(), 2);
            assert_eq!(results.len(), 1, "Expected 1 result, got: {:?}", results);
        } else {
            panic!("Expected Call");
        }
    }

    #[test]
    fn test_parse_call_no_results() {
        let input = "call %func(v0)";
        let result = parse_call(input);
        assert!(result.is_ok(), "parse_call failed: {:?}", result);
        let (_, inst) = result.unwrap();
        if let Inst::Call { results, .. } = inst {
            assert_eq!(results.len(), 0);
        } else {
            panic!("Expected Call");
        }
    }

    #[test]
    fn test_parse_syscall() {
        let input = "syscall 1(v0, v1)";
        let result = parse_syscall(input);
        assert!(result.is_ok(), "parse_syscall failed: {:?}", result);
        let (_, inst) = result.unwrap();
        if let Inst::Syscall { number, args } = inst {
            assert_eq!(number, 1);
            assert_eq!(args.len(), 2);
        } else {
            panic!("Expected Syscall");
        }
    }

    #[test]
    fn test_parse_load() {
        let input = "v0 = load.i32 v1";
        let result = parse_load(input);
        assert!(result.is_ok(), "parse_load failed: {:?}", result);
        let (_, inst) = result.unwrap();
        if let Inst::Load { ty, .. } = inst {
            assert_eq!(ty, crate::Type::I32);
        } else {
            panic!("Expected Load");
        }
    }

    #[test]
    fn test_parse_store() {
        let input = "store.i32 v0, v1";
        let result = parse_store(input);
        assert!(result.is_ok(), "parse_store failed: {:?}", result);
        let (_, inst) = result.unwrap();
        if let Inst::Store { ty, .. } = inst {
            assert_eq!(ty, crate::Type::I32);
        } else {
            panic!("Expected Store");
        }
    }

    #[test]
    fn test_parse_halt() {
        let input = "halt";
        let result = parse_halt(input);
        assert!(result.is_ok(), "parse_halt failed: {:?}", result);
        let (_, inst) = result.unwrap();
        assert!(matches!(inst, Inst::Halt));
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
        let (remaining, inst) = result.unwrap();
        // Should consume the call and whitespace, leaving the next instruction
        assert!(
            remaining.trim_start().starts_with("v12"),
            "Should leave next instruction, got: {:?}",
            remaining
        );
        if let Inst::Call {
            callee,
            args,
            results,
        } = inst
        {
            assert_eq!(callee, "helper");
            assert_eq!(args.len(), 1);
            assert_eq!(results.len(), 1);
        } else {
            panic!("Expected Call instruction");
        }
    }

    #[test]
    fn test_parse_call_followed_by_instruction() {
        // Test parsing call followed by another instruction
        let input = "call %helper(v10) -> v11\n        v12 = iconst 100";
        let result = parse_instruction(input);
        assert!(result.is_ok(), "parse_instruction failed: {:?}", result);
        let (remaining, inst) = result.unwrap();
        assert!(
            remaining.trim_start().starts_with("v12"),
            "Should leave next instruction, got: {:?}",
            remaining
        );
        assert!(matches!(inst, Inst::Call { .. }));
    }

    #[test]
    fn test_parse_call_multiple_returns() {
        // Test call with multiple return values
        let input = "call %func(v0, v1) -> v2, v3, v4";
        let result = parse_call(input);
        assert!(result.is_ok(), "parse_call failed: {:?}", result);
        let (remaining, inst) = result.unwrap();
        assert_eq!(
            remaining, "",
            "Should consume all input, got: {:?}",
            remaining
        );
        if let Inst::Call {
            callee,
            args,
            results,
        } = inst
        {
            assert_eq!(callee, "func");
            assert_eq!(args.len(), 2);
            assert_eq!(results.len(), 3, "Expected 3 results, got: {:?}", results);
        } else {
            panic!("Expected Call");
        }
    }

    #[test]
    fn test_parse_call_multiple_returns_via_instruction() {
        // Test parsing call with multiple returns via parse_instruction
        let input = "call %helper(v10) -> v11, v12, v13";
        let result = parse_instruction(input);
        assert!(result.is_ok(), "parse_instruction failed: {:?}", result);
        let (remaining, inst) = result.unwrap();
        assert_eq!(remaining, "", "Should consume all input");
        if let Inst::Call { results, .. } = inst {
            assert_eq!(results.len(), 3, "Expected 3 results, got: {:?}", results);
        } else {
            panic!("Expected Call instruction");
        }
    }

    #[test]
    fn test_parse_return_multiple_values() {
        // Test return with multiple values (more than 2)
        let input = "return v0 v1 v2 v3";
        let result = parse_return(input);
        assert!(result.is_ok());
        let (_, inst) = result.unwrap();
        if let Inst::Return { values } = inst {
            assert_eq!(values.len(), 4, "Expected 4 values, got: {:?}", values);
        } else {
            panic!("Expected Return with 4 values");
        }
    }

    #[test]
    fn test_parse_return_multiple_values_via_instruction() {
        // Test parsing return with multiple values via parse_instruction
        let input = "return v0 v1 v2 v3 v4";
        let result = parse_instruction(input);
        assert!(result.is_ok(), "parse_instruction failed: {:?}", result);
        let (remaining, inst) = result.unwrap();
        assert_eq!(remaining, "", "Should consume all input");
        if let Inst::Return { values } = inst {
            assert_eq!(values.len(), 5, "Expected 5 values, got: {:?}", values);
        } else {
            panic!("Expected Return instruction");
        }
    }

    #[test]
    fn test_parse_return_empty() {
        // Test that return without values is valid
        let result = parse_instruction("return");
        assert!(result.is_ok(), "Return without values should be valid");
    }
}
