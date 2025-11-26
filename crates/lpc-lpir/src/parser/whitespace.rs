//! Whitespace parsing utilities.

use nom::{
    branch::alt,
    bytes::complete::{tag, take_while},
    character::complete::{char, multispace1},
    combinator::{map, recognize},
    multi::many0,
    sequence::preceded,
    IResult,
};

/// Parse a single-line comment starting with `;`
/// Consumes the `;` and everything until (but not including) the newline
/// If there's no newline (end of input), consumes everything after `;`
pub(crate) fn comment(input: &str) -> IResult<&str, &str> {
    preceded(char(';'), take_while(|c| c != '\n' && c != '\r'))(input)
}

/// Parse whitespace (spaces, tabs, newlines, comments) - returns the matched string
pub(crate) fn blank_space(input: &str) -> IResult<&str, &str> {
    recognize(many0(alt((
        map(multispace1, |_| ()),
        map(tag("\\\n"), |_| ()), // Line continuation
        map(comment, |_| ()),     // Single-line comments
    ))))(input)
}

/// Parse whitespace and discard result - returns ()
/// This is the main whitespace parser to use throughout
pub(crate) fn blank(input: &str) -> IResult<&str, ()> {
    map(blank_space, |_| ())(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blank_space() {
        assert_eq!(blank_space("   "), Ok(("", "   ")));
        assert_eq!(blank_space("\n\t  "), Ok(("", "\n\t  ")));
        assert_eq!(blank_space("  v0"), Ok(("v0", "  ")));
        assert_eq!(blank_space(""), Ok(("", "")));
    }

    #[test]
    fn test_blank() {
        assert_eq!(blank("   "), Ok(("", ())));
        assert_eq!(blank("\n\t  "), Ok(("", ())));
        assert_eq!(blank("  v0"), Ok(("v0", ())));
        assert_eq!(blank(""), Ok(("", ())));
    }

    #[test]
    fn test_comment() {
        // Single-line comment with text
        assert_eq!(comment("; comment\n"), Ok(("\n", " comment")));
        assert_eq!(
            comment("; this is a comment\nrest"),
            Ok(("\nrest", " this is a comment"))
        );

        // Empty comment
        assert_eq!(comment(";\n"), Ok(("\n", "")));
        // Comment at end of input (no newline) - consumes everything after ;
        assert_eq!(comment(";rest"), Ok(("", "rest")));

        // Comment with only whitespace
        assert_eq!(comment(";   \n"), Ok(("\n", "   ")));

        // Comment at end of input (no newline)
        assert_eq!(comment("; comment"), Ok(("", " comment")));
        assert_eq!(comment(";"), Ok(("", "")));

        // Comment with Windows line ending
        assert_eq!(comment("; comment\r\n"), Ok(("\r\n", " comment")));
    }

    #[test]
    fn test_blank_space_with_comments() {
        // Comment alone
        assert_eq!(blank_space("; comment\n"), Ok(("", "; comment\n")));

        // Comment with whitespace
        assert_eq!(blank_space("  ; comment\n"), Ok(("", "  ; comment\n")));
        assert_eq!(blank_space("; comment\n  "), Ok(("", "; comment\n  ")));

        // Multiple comments
        assert_eq!(
            blank_space("; first\n; second\n"),
            Ok(("", "; first\n; second\n"))
        );

        // Comment between tokens
        assert_eq!(blank_space("  ; comment\n  "), Ok(("", "  ; comment\n  ")));

        // Comment at end of input
        assert_eq!(blank_space("; comment"), Ok(("", "; comment")));
    }

    #[test]
    fn test_blank_with_comments() {
        // Comment should be treated as whitespace
        assert_eq!(blank("; comment\n"), Ok(("", ())));
        assert_eq!(blank("  ; comment\n  "), Ok(("", ())));
        assert_eq!(blank("; comment"), Ok(("", ())));

        // Comment followed by token
        let result = blank("; comment\nv0");
        assert!(result.is_ok());
        let (remaining, _) = result.unwrap();
        assert_eq!(remaining, "v0");
    }
}
