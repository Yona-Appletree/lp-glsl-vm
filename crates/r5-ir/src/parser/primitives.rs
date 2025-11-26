//! Primitive parsers for types, values, names, and literals.

use alloc::string::{String, ToString};

use nom::{
    branch::alt,
    bytes::complete::{tag, take_while1},
    character::complete::char,
    combinator::{map, map_res, opt, recognize},
    number::complete::double,
    sequence::{pair, preceded},
    IResult,
};

use crate::{types::Type, value::Value};

/// Parse an integer literal
pub(crate) fn integer(input: &str) -> IResult<&str, i64> {
    map_res(
        recognize(pair(
            opt(char('-')),
            take_while1(|c: char| c.is_ascii_digit()),
        )),
        |s: &str| s.parse::<i64>(),
    )(input)
}

/// Parse a float literal
pub(crate) fn float(input: &str) -> IResult<&str, f64> {
    double(input)
}

/// Parse a type (i32, i64, f32, f64)
pub(crate) fn parse_type(input: &str) -> IResult<&str, Type> {
    alt((
        map(tag("i32"), |_| Type::I32),
        map(tag("i64"), |_| Type::I64),
        map(tag("f32"), |_| Type::F32),
        map(tag("f64"), |_| Type::F64),
    ))(input)
}

/// Parse a value (v0, v1, etc.)
pub(crate) fn parse_value(input: &str) -> IResult<&str, Value> {
    map(
        map_res(
            preceded(char('v'), take_while1(|c: char| c.is_ascii_digit())),
            |s: &str| s.parse::<u32>(),
        ),
        Value::new,
    )(input)
}

/// Parse a function name (%name)
pub(crate) fn parse_function_name(input: &str) -> IResult<&str, String> {
    map(
        preceded(
            char('%'),
            take_while1(|c: char| c.is_alphanumeric() || c == '_'),
        ),
        |s: &str| s.to_string(),
    )(input)
}

/// Parse a block index (block0, block1, etc.)
pub(crate) fn parse_block_index(input: &str) -> IResult<&str, u32> {
    map_res(
        preceded(tag("block"), take_while1(|c: char| c.is_ascii_digit())),
        |s: &str| s.parse::<u32>(),
    )(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Type;

    #[test]
    fn test_integer() {
        assert_eq!(integer("42"), Ok(("", 42)));
        assert_eq!(integer("-42"), Ok(("", -42)));
        assert_eq!(integer("0"), Ok(("", 0)));
        assert_eq!(integer("42 "), Ok((" ", 42)));
    }

    #[test]
    fn test_float() {
        assert_eq!(float("3.14"), Ok(("", 3.14)));
        assert_eq!(float("-3.14"), Ok(("", -3.14)));
        assert_eq!(float("0.0"), Ok(("", 0.0)));
    }

    #[test]
    fn test_parse_type() {
        assert_eq!(parse_type("i32"), Ok(("", Type::I32)));
        assert_eq!(parse_type("i64"), Ok(("", Type::I64)));
        assert_eq!(parse_type("f32"), Ok(("", Type::F32)));
        assert_eq!(parse_type("f64"), Ok(("", Type::F64)));
    }

    #[test]
    fn test_parse_value() {
        assert_eq!(parse_value("v0"), Ok(("", Value::new(0))));
        assert_eq!(parse_value("v42"), Ok(("", Value::new(42))));
        assert_eq!(parse_value("v0 "), Ok((" ", Value::new(0))));
    }

    #[test]
    fn test_parse_function_name() {
        assert_eq!(parse_function_name("%add"), Ok(("", "add".to_string())));
        assert_eq!(
            parse_function_name("%test_func"),
            Ok(("", "test_func".to_string()))
        );
        assert_eq!(parse_function_name("%main "), Ok((" ", "main".to_string())));
    }

    #[test]
    fn test_parse_block_index() {
        assert_eq!(parse_block_index("block0"), Ok(("", 0)));
        assert_eq!(parse_block_index("block42"), Ok(("", 42)));
        assert_eq!(parse_block_index("block1 "), Ok((" ", 1)));
    }

    #[test]
    fn test_integer_overflow() {
        // Test that very large integers fail to parse (would overflow i64)
        let result = integer("999999999999999999999999999999999999999");
        assert!(result.is_err(), "Should fail on overflow");
    }

    #[test]
    fn test_integer_invalid() {
        // Test that non-numeric input fails
        let result = integer("abc");
        assert!(result.is_err(), "Should fail on non-numeric input");
    }

    #[test]
    fn test_parse_value_invalid() {
        // Test that missing 'v' prefix fails
        let result = parse_value("0");
        assert!(result.is_err(), "Should fail without 'v' prefix");
    }

    #[test]
    fn test_parse_value_overflow() {
        // Test that very large value indices fail to parse (would overflow u32)
        let result = parse_value("v999999999999999999999");
        assert!(result.is_err(), "Should fail on overflow");
    }

    #[test]
    fn test_parse_block_index_invalid() {
        // Test that missing 'block' prefix fails
        let result = parse_block_index("0");
        assert!(result.is_err(), "Should fail without 'block' prefix");
    }

    #[test]
    fn test_parse_block_index_overflow() {
        // Test that very large block indices fail to parse (would overflow u32)
        let result = parse_block_index("block999999999999999999999");
        assert!(result.is_err(), "Should fail on overflow");
    }

    #[test]
    fn test_parse_type_invalid() {
        // Test that invalid type fails
        let result = parse_type("invalid");
        assert!(result.is_err(), "Should fail on invalid type");
    }

    #[test]
    fn test_parse_function_name_invalid() {
        // Test that missing '%' prefix fails
        let result = parse_function_name("name");
        assert!(result.is_err(), "Should fail without '%' prefix");
    }

    #[test]
    fn test_parse_function_name_empty() {
        // Test that empty name after '%' fails
        let result = parse_function_name("%");
        assert!(result.is_err(), "Should fail on empty name");
    }
}
