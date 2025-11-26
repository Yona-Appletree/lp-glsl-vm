//! Module parser.

use alloc::string::String;

use nom::{
    bytes::complete::tag,
    character::complete::char,
    combinator::opt,
    multi::many0,
    sequence::{preceded, terminated},
    IResult,
};

use super::{
    function::parse_function_internal, primitives::parse_function_name, whitespace::blank,
};
use crate::module::Module;

/// Parse an entry declaration: entry: %name
/// Handles its own leading whitespace (for use in opt)
fn parse_entry(input: &str) -> IResult<&str, String> {
    let (input, _) = terminated(tag("entry"), blank)(input)?;
    let (input, _) = terminated(char(':'), blank)(input)?;
    let (input, name) = terminated(parse_function_name, blank)(input)?;
    Ok((input, name))
}

/// Parse a module (internal)
pub(crate) fn parse_module_internal(input: &str) -> IResult<&str, Module> {
    let (input, _) = terminated(tag("module"), blank)(input)?;
    let (input, _) = terminated(char('{'), blank)(input)?;

    let (input, entry) = opt(parse_entry)(input)?;

    // Consume whitespace before each function (many0 doesn't do this automatically)
    let (input, functions) = many0(parse_function_internal)(input)?;

    let (input, _) = terminated(char('}'), blank)(input)?;

    let mut module = Module::new();

    // Add all functions to module
    let mut anon_counter = 0;
    for mut func in functions {
        if let Some(name) = &func.name {
            module.add_function(name.clone(), func);
        } else {
            // Generate a unique name for unnamed functions
            let anon_name = alloc::format!("anon_{}", anon_counter);
            anon_counter += 1;
            func.set_name(anon_name.clone());
            module.add_function(anon_name, func);
        }
    }

    // Set entry function if specified
    if let Some(entry_name) = entry {
        module.set_entry_function(entry_name);
    }

    Ok((input, module))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_entry() {
        let input = "entry: %main";
        let result = parse_entry(input);
        assert!(result.is_ok());
        let (_, name) = result.unwrap();
        assert_eq!(name, "main");
    }

    #[test]
    fn test_parse_module_internal() {
        let input = r#"module {
            entry: %main

            function %main() {
            block0:
                v0 = iconst 42
                return v0
            }
        }"#;
        let result = parse_module_internal(input.trim());
        assert!(result.is_ok(), "parse_module_internal failed: {:?}", result);
        let (remaining, module) = result.unwrap();
        assert_eq!(remaining, "", "Should consume all input");
        assert_eq!(module.function_count(), 1);
        assert!(module.entry_function().is_some());
    }

    #[test]
    fn test_parse_module_with_unnamed_function() {
        // Test that unnamed functions get auto-generated names
        let input = r#"module {
            function() {
            block0:
                return
            }
        }"#;
        let result = parse_module_internal(input.trim());
        assert!(result.is_ok(), "parse_module_internal failed: {:?}", result);
        let (remaining, module) = result.unwrap();
        assert_eq!(remaining, "", "Should consume all input");
        assert_eq!(module.function_count(), 1);
        // Check that the function has an auto-generated name
        let func_name = module.functions.iter().next().unwrap().0;
        assert!(
            func_name.starts_with("anon_"),
            "Function should have auto-generated name"
        );
    }

    #[test]
    fn test_parse_module_multiple_unnamed_functions() {
        // Test that multiple unnamed functions get unique names
        let input = r#"module {
            function() {
            block0:
                return
            }
            function() {
            block0:
                return
            }
        }"#;
        let result = parse_module_internal(input.trim());
        assert!(result.is_ok(), "parse_module_internal failed: {:?}", result);
        let (remaining, module) = result.unwrap();
        assert_eq!(remaining, "", "Should consume all input");
        assert_eq!(module.function_count(), 2);
        // Check that functions have unique auto-generated names
        let names: alloc::vec::Vec<_> = module
            .functions
            .iter()
            .map(|(name, _)| name.as_str())
            .collect();
        assert_eq!(names[0], "anon_0");
        assert_eq!(names[1], "anon_1");
    }

    #[test]
    fn test_parse_module_two_named_functions() {
        // Test parsing two named functions with whitespace between them
        // This reproduces the issue from test_prologue_adjusts_sp_once
        // The key issue: after parsing the first function's closing brace,
        // there's whitespace/newlines before the second function starts
        let input = r#"module {
    function %helper(i32) -> i32 {
    block0(v0: i32):
        v1 = iconst 1
        v2 = iadd v0, v1
        return v2
    }

    function %main(i32) -> i32 {
    block0(v0: i32):
        v1 = iconst 1
        v2 = iadd v0, v1
        v3 = iconst 2
        v4 = iadd v2, v3
        v5 = iconst 3
        v6 = iadd v4, v5
        v7 = iconst 4
        v8 = iadd v6, v7
        v9 = iconst 5
        v10 = iadd v8, v9
        call %helper(v10) -> v11
        v12 = iconst 100
        v13 = iadd v11, v12
        return v13
    }
}"#;
        let result = parse_module_internal(input.trim());
        assert!(
            result.is_ok(),
            "parse_module_internal failed: {:?}\nThis test should pass after fixing whitespace \
             handling in many0(parse_function_internal).\nThe issue is that whitespace between \
             functions isn't being consumed.",
            result
        );
        let (remaining, module) = result.unwrap();
        assert_eq!(
            remaining,
            "",
            "Should consume all input, but {} bytes remaining",
            remaining.len()
        );
        assert_eq!(
            module.function_count(),
            2,
            "Should have 2 functions, but found {}",
            module.function_count()
        );
        assert!(
            module.functions.contains_key("helper"),
            "Should contain 'helper' function"
        );
        assert!(
            module.functions.contains_key("main"),
            "Should contain 'main' function"
        );
    }
}
