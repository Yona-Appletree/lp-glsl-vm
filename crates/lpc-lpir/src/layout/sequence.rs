//! Sequence numbers for program order comparison.
//!
//! Sequence numbers are assigned like BASIC line numbers (10, 20, 30...)
//! to allow O(1) program order comparison while leaving room for insertions.

/// Sequence number type (BASIC-style: 10, 20, 30...)
pub type SequenceNumber = u32;

/// Initial stride for sequence numbers
pub const MAJOR_STRIDE: SequenceNumber = 10;

/// Minor stride for renumbering when space runs out
pub const MINOR_STRIDE: SequenceNumber = 2;

/// Limit for local renumbering before full block renumber
pub const LOCAL_LIMIT: SequenceNumber = 100 * MINOR_STRIDE;

/// Compute midpoint between two sequence numbers
///
/// Returns `None` if there's no room between the numbers (they're too close).
/// This is used when inserting instructions to find a sequence number
/// between the previous and next instruction.
pub fn midpoint(a: SequenceNumber, b: SequenceNumber) -> Option<SequenceNumber> {
    debug_assert!(a < b, "midpoint: a ({}) must be less than b ({})", a, b);

    // Avoid integer overflow
    let m = a + (b - a) / 2;

    // Return None if midpoint would be equal to either endpoint
    if m > a {
        Some(m)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_midpoint_basic() {
        assert_eq!(midpoint(0, 2), Some(1));
        assert_eq!(midpoint(0, 4), Some(2));
        assert_eq!(midpoint(10, 30), Some(20));
        assert_eq!(midpoint(10, 20), Some(15));
    }

    #[test]
    fn test_midpoint_edge_cases() {
        // No room between consecutive numbers
        assert_eq!(midpoint(0, 1), None);
        assert_eq!(midpoint(3, 4), None);

        // Large numbers
        assert_eq!(midpoint(1000, 2000), Some(1500));
    }

    #[test]
    fn test_midpoint_ordering() {
        let a = 10;
        let b = 30;
        if let Some(m) = midpoint(a, b) {
            assert!(m > a);
            assert!(m < b);
        }
    }

    #[test]
    #[should_panic(expected = "must be less than")]
    fn test_midpoint_panic_on_equal() {
        midpoint(10, 10);
    }

    #[test]
    #[should_panic(expected = "must be less than")]
    fn test_midpoint_panic_on_reversed() {
        midpoint(20, 10);
    }
}

