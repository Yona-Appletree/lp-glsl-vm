//! Function and signature parsers.

use nom::{
    bytes::complete::tag,
    character::complete::char,
    combinator::{map, opt},
    multi::separated_list0,
    sequence::{delimited, terminated, tuple},
    IResult,
};

use super::{
    block::parse_block,
    inst_converter::inst_to_inst_data,
    primitives::{parse_function_name, parse_type},
    whitespace::blank,
};
use crate::{function::Function, signature::Signature};

/// Parse a function signature: (i32, i32) -> i32
pub(crate) fn parse_signature(input: &str) -> IResult<&str, Signature> {
    let (input, params) = delimited(
        terminated(char('('), blank),
        separated_list0(terminated(char(','), blank), terminated(parse_type, blank)),
        terminated(char(')'), blank),
    )(input)?;

    let (input, returns) = opt(map(
        tuple((
            blank,
            tag("->"),
            blank,
            separated_list0(terminated(char(','), blank), terminated(parse_type, blank)),
        )),
        |(_, _, _, types)| types,
    ))(input)?;

    Ok((
        input,
        Signature {
            params,
            returns: returns.unwrap_or_default(),
        },
    ))
}

/// Parse a function (internal, used by module parser)
/// The module parser handles leading whitespace before calling this
/// If no name is provided, a temporary name will be generated (module parser will replace it)
pub(crate) fn parse_function_internal(input: &str, anon_counter: usize) -> IResult<&str, Function> {
    let (input, _) = terminated(tag("function"), blank)(input)?;
    let (input, name) = opt(terminated(parse_function_name, blank))(input)?;
    let (input, signature) = parse_signature(input)?;
    let (input, _) = terminated(char('{'), blank)(input)?;

    // Generate a name if none was provided (module parser will replace with proper anon name)
    let name = name.unwrap_or_else(|| alloc::format!("temp_anon_{}", anon_counter));

    // Create function with new API
    let mut function = Function::new(signature, name);

    // Parse blocks and build function incrementally
    let mut input = input;
    loop {
        // Check if we're at the closing brace
        let (remaining, _) = blank(input)?;
        if remaining.starts_with('}') {
            break;
        }

        // Parse a block
        let (remaining, (params, insts)) = parse_block(remaining)?;
        input = remaining;

        // Create block in function
        let block_entity = if params.is_empty() {
            function.create_block()
        } else {
            function.create_block_with_params(params)
        };
        function.append_block(block_entity);

        // Add instructions to the block
        for inst in insts {
            let inst_data = inst_to_inst_data(inst);
            let inst_entity = function.create_inst(inst_data);
            function.append_inst(inst_entity, block_entity);
        }
    }

    // Allow whitespace before closing brace
    let (input, _) = terminated(char('}'), blank)(input)?;

    Ok((input, function))
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::*;

    #[test]
    fn test_parse_signature() {
        let input = "() -> i32";
        let result = parse_signature(input);
        assert!(result.is_ok(), "parse_signature failed: {:?}", result);
        let (remaining, sig) = result.unwrap();
        assert_eq!(remaining, "", "Should consume all input");
        assert_eq!(sig.params.len(), 0);
        assert_eq!(sig.returns.len(), 1);
    }

    #[test]
    fn test_parse_signature_with_params() {
        let input = "(i32, i32) -> i32";
        let result = parse_signature(input);
        assert!(result.is_ok());
        let (_, sig) = result.unwrap();
        assert_eq!(sig.params.len(), 2);
        assert_eq!(sig.returns.len(), 1);
    }

    #[test]
    fn test_parse_function_internal_minimal() {
        let input = "function %test() -> i32 {\nblock0:\n    v0 = iconst 42\n    return v0\n}";
        let result = parse_function_internal(input, 0);
        assert!(
            result.is_ok(),
            "parse_function_internal failed: {:?}",
            result
        );
        let (remaining, func) = result.unwrap();
        assert_eq!(
            remaining, "",
            "Should consume all input, got: {:?}",
            remaining
        );
        assert_eq!(func.block_count(), 1);
        let block = func.entry_block().unwrap();
        let insts: Vec<_> = func.block_insts(block).collect();
        assert_eq!(insts.len(), 2);
    }

    #[test]
    fn test_parse_signature_multiple_returns() {
        let input = "() -> i32, i32, i32";
        let result = parse_signature(input);
        assert!(result.is_ok(), "parse_signature failed: {:?}", result);
        let (remaining, sig) = result.unwrap();
        assert_eq!(remaining, "", "Should consume all input");
        assert_eq!(sig.params.len(), 0);
        assert_eq!(sig.returns.len(), 3, "Expected 3 return types");
    }

    #[test]
    fn test_parse_signature_many_returns() {
        let input = "(i32, i32) -> i32, i32, i32, i32, i32, i32, i32, i32, i32, i32";
        let result = parse_signature(input);
        assert!(result.is_ok(), "parse_signature failed: {:?}", result);
        let (remaining, sig) = result.unwrap();
        assert_eq!(remaining, "", "Should consume all input");
        assert_eq!(sig.params.len(), 2);
        assert_eq!(sig.returns.len(), 10, "Expected 10 return types");
    }

    #[test]
    fn test_parse_function_with_multiple_returns() {
        let input = r#"function %test() -> i32, i32 {
block0:
    v0 = iconst 1
    v1 = iconst 2
    return v0 v1
}"#;
        let result = parse_function_internal(input, 0);
        assert!(
            result.is_ok(),
            "parse_function_internal failed: {:?}",
            result
        );
        let (remaining, func) = result.unwrap();
        assert_eq!(remaining, "", "Should consume all input");
        assert_eq!(func.signature.returns.len(), 2);
    }
}
