//! Block parser.

use alloc::vec::Vec;

use nom::{
    character::complete::char,
    combinator::opt,
    multi::{many0, separated_list0},
    sequence::{delimited, preceded, terminated},
    IResult,
};

use super::{
    instructions::parse_instruction,
    primitives::{parse_block_index, parse_type, parse_value},
    whitespace::blank,
};
use crate::block::Block;

/// Parse a single block parameter: v0: i32
/// Handles its own leading whitespace (for use in separated_list0)
fn parse_block_param(input: &str) -> IResult<&str, crate::value::Value> {
    let (input, _) = blank(input)?;
    let (input, value) = terminated(parse_value, blank)(input)?;
    let (input, _) = terminated(char(':'), blank)(input)?;
    let (input, _ty) = terminated(parse_type, blank)(input)?;
    Ok((input, value))
}

/// Parse a list of block parameters: v0: i32, v1: i32
/// Uses terminated on the item to match GLSL pattern: both separator and item consume trailing whitespace
fn parse_block_param_list(input: &str) -> IResult<&str, Vec<crate::value::Value>> {
    separated_list0(
        terminated(char(','), blank),
        terminated(parse_block_param, blank),
    )(input)
}

/// Parse block parameters: (v0: i32, v1: i32)
fn parse_block_params(input: &str) -> IResult<&str, Vec<crate::value::Value>> {
    delimited(
        terminated(char('('), blank),
        parse_block_param_list,
        preceded(blank, char(')')),
    )(input)
}

/// Parse a block
pub(crate) fn parse_block(input: &str) -> IResult<&str, Block> {
    let (input, _) = blank(input)?;
    let (input, _block_index) = terminated(parse_block_index, blank)(input)?;
    let (input, params) = opt(parse_block_params)(input)?;
    let (input, _) = terminated(char(':'), blank)(input)?;

    // Parse instructions - many0 will stop when it can't parse more
    // Instructions are terminated with blank, so they'll naturally stop
    // when we hit a new block or closing brace
    let (input, insts) = many0(terminated(parse_instruction, blank))(input)?;

    Ok((
        input,
        Block {
            params: params.unwrap_or_default(),
            insts,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_block_param() {
        let input = "v0: i32";
        let result = parse_block_param(input);
        assert!(result.is_ok(), "parse_block_param failed: {:?}", result);
        let (remaining, value) = result.unwrap();
        assert_eq!(remaining, "");
        assert_eq!(value.index(), 0);
    }

    #[test]
    fn test_parse_block_param_with_whitespace() {
        let input = "v0 : i32 ";
        let result = parse_block_param(input);
        assert!(result.is_ok(), "parse_block_param failed: {:?}", result);
        let (remaining, value) = result.unwrap();
        assert_eq!(remaining, "");
        assert_eq!(value.index(), 0);
    }

    #[test]
    fn test_parse_block_param_list_single() {
        let input = "v0: i32";
        let result = parse_block_param_list(input);
        assert!(
            result.is_ok(),
            "parse_block_param_list failed: {:?}",
            result
        );
        let (remaining, params) = result.unwrap();
        assert_eq!(remaining, "");
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].index(), 0);
    }

    #[test]
    fn test_parse_block_param_list_multiple() {
        let input = "v0: i32, v1: i32";
        let result = parse_block_param_list(input);
        assert!(
            result.is_ok(),
            "parse_block_param_list failed: {:?}",
            result
        );
        let (remaining, params) = result.unwrap();
        assert_eq!(remaining, "");
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].index(), 0);
        assert_eq!(params[1].index(), 1);
    }

    #[test]
    fn test_parse_block_params_empty() {
        let input = "()";
        let result = parse_block_params(input);
        assert!(result.is_ok(), "parse_block_params failed: {:?}", result);
        let (remaining, params) = result.unwrap();
        assert_eq!(remaining, "");
        assert_eq!(params.len(), 0);
    }

    #[test]
    fn test_parse_block_params_single() {
        let input = "(v0: i32)";
        let result = parse_block_params(input);
        assert!(result.is_ok(), "parse_block_params failed: {:?}", result);
        let (remaining, params) = result.unwrap();
        assert_eq!(remaining, "");
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].index(), 0);
    }

    #[test]
    fn test_parse_block_params_multiple() {
        let input = "(v0: i32, v1: i32)";
        let result = parse_block_params(input);
        assert!(result.is_ok(), "parse_block_params failed: {:?}", result);
        let (remaining, params) = result.unwrap();
        assert_eq!(remaining, "");
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].index(), 0);
        assert_eq!(params[1].index(), 1);
    }

    #[test]
    fn test_parse_block_params_with_whitespace() {
        let input = "( v0 : i32 , v1 : i32 )";
        let result = parse_block_params(input);
        assert!(result.is_ok(), "parse_block_params failed: {:?}", result);
        let (remaining, params) = result.unwrap();
        assert_eq!(remaining, "");
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_parse_block_simple() {
        let input = "block0:\n    v0 = iconst 42\n    return v0";
        let result = parse_block(input);
        assert!(result.is_ok(), "parse_block failed: {:?}", result);
        let (_remaining, block) = result.unwrap();
        assert_eq!(block.insts.len(), 2, "Expected 2 instructions");
        assert!(matches!(block.insts[0], crate::inst::Inst::Iconst { .. }));
        assert!(matches!(block.insts[1], crate::inst::Inst::Return { .. }));
    }

    #[test]
    fn test_parse_block_with_params() {
        let input = "block0(v0: i32, v1: i32):\n    v2 = iadd v0, v1\n    return v2";
        let result = parse_block(input);
        assert!(result.is_ok(), "parse_block failed: {:?}", result);
        let (_, block) = result.unwrap();
        assert_eq!(block.params.len(), 2);
        assert_eq!(block.insts.len(), 2);
    }

    #[test]
    fn test_parse_block_step_by_step() {
        // Test parsing step by step to isolate the issue
        let input = "block0(v0: i32, v1: i32):";

        // Step 1: Parse block index
        let (input1, _idx) = parse_block_index(input).unwrap();
        assert_eq!(input1, "(v0: i32, v1: i32):");

        // Step 2: Parse block params
        let (input2, params) = parse_block_params(input1).unwrap();
        assert_eq!(params.len(), 2);
        assert_eq!(input2, ":");

        // Step 3: Verify colon is there
        assert!(input2.starts_with(':'));
    }

    #[test]
    fn test_parse_block_missing_colon() {
        // Test that missing colon after block index fails
        let input = "block0\n    v0 = iconst 42";
        let result = parse_block(input);
        assert!(result.is_err(), "Should fail without colon");
    }

    #[test]
    fn test_parse_block_malformed_params() {
        // Test that malformed parameters fail
        let input = "block0(v0: i32, v1:):\n    v2 = iconst 42";
        let result = parse_block(input);
        assert!(result.is_err(), "Should fail on malformed parameters");
    }

    #[test]
    fn test_parse_block_params_missing_type() {
        // Test that missing type in parameter fails
        let input = "block0(v0:):\n    v1 = iconst 42";
        let result = parse_block(input);
        assert!(result.is_err(), "Should fail on missing type");
    }

    #[test]
    fn test_parse_block_params_missing_value() {
        // Test that missing value in parameter fails
        let input = "block0(: i32):\n    v0 = iconst 42";
        let result = parse_block(input);
        assert!(result.is_err(), "Should fail on missing value");
    }

    #[test]
    fn test_parse_block_empty() {
        // Test that empty block (no instructions) is valid
        let input = "block0:";
        let result = parse_block(input);
        assert!(result.is_ok(), "Empty block should be valid");
        let (_, block) = result.unwrap();
        assert_eq!(block.insts.len(), 0);
    }
}
