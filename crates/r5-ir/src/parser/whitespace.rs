//! Whitespace parsing utilities.

use nom::{
    branch::alt,
    bytes::complete::tag,
    character::complete::multispace1,
    combinator::{map, recognize},
    multi::many0,
    IResult,
};

/// Parse whitespace (spaces, tabs, newlines) - returns the matched string
pub(crate) fn blank_space(input: &str) -> IResult<&str, &str> {
    recognize(many0(alt((
        map(multispace1, |_| ()),
        map(tag("\\\n"), |_| ()), // Line continuation
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
}
