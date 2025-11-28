//! Source location tracking for IR instructions.
//!
//! This module provides source location tracking similar to Cranelift's design,
//! allowing instructions to track their original source code positions for
//! debugging and correlation between source code and generated machine code.

use core::fmt;

/// Opaque source location identifier.
///
/// This is an opaque u32 that can encode file/line/column information
/// however the frontend wants. The default value `!0` (all-ones) represents
/// an invalid/unknown source location.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceLoc(u32);

impl SourceLoc {
    /// Create a new source location from raw bits.
    pub fn new(bits: u32) -> Self {
        SourceLoc(bits)
    }

    /// Get the raw bits of this source location.
    pub fn bits(self) -> u32 {
        self.0
    }

    /// Check if this is the default (invalid) source location.
    pub fn is_default(self) -> bool {
        self.0 == !0
    }
}

impl Default for SourceLoc {
    fn default() -> Self {
        SourceLoc(!0)
    }
}

impl fmt::Display for SourceLoc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_default() {
            write!(f, "srcloc(unknown)")
        } else {
            write!(f, "srcloc({})", self.0)
        }
    }
}

/// Relative source location for efficiency.
///
/// Stores an offset relative to a base source location. This allows
/// efficient storage when many instructions come from nearby source locations.
///
/// Note: Offset 0 can mean either "same as base" or "no source location".
/// We use i32::MIN as a sentinel to represent "no source location" to distinguish
/// it from a valid offset of 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RelSourceLoc {
    /// Offset relative to base source location (in bytes or lines, depending on encoding)
    /// Special value: i32::MIN represents "no source location"
    offset: i32,
}

impl RelSourceLoc {
    /// Create a new relative source location with the given offset.
    pub fn new(offset: i32) -> Self {
        RelSourceLoc { offset }
    }

    /// Create a relative source location from a base and absolute source location.
    ///
    /// Computes the offset from the base to the absolute location.
    pub fn from_base_offset(base: SourceLoc, absolute: SourceLoc) -> Self {
        if absolute.is_default() {
            // If absolute is default, use sentinel to represent "no source location"
            RelSourceLoc {
                offset: i32::MIN,
            }
        } else if base.is_default() {
            // If base is default but absolute is not, we can't compute a meaningful offset
            // Store as sentinel to represent "no source location"
            RelSourceLoc {
                offset: i32::MIN,
            }
        } else {
            // Compute offset (wrapping arithmetic to handle underflow)
            let base_bits = base.bits() as i32;
            let abs_bits = absolute.bits() as i32;
            RelSourceLoc {
                offset: abs_bits.wrapping_sub(base_bits),
            }
        }
    }

    /// Expand this relative source location to an absolute source location
    /// given a base source location.
    pub fn expand(self, base: SourceLoc) -> SourceLoc {
        // Check for sentinel value representing "no source location"
        if self.offset == i32::MIN {
            return SourceLoc::default();
        }
        if base.is_default() {
            SourceLoc::default()
        } else {
            let base_bits = base.bits() as i32;
            let result = base_bits.wrapping_add(self.offset);
            SourceLoc::new(result as u32)
        }
    }

    /// Check if this is the default (zero offset or sentinel) relative source location.
    pub fn is_default(self) -> bool {
        self.offset == 0 || self.offset == i32::MIN
    }
}

impl Default for RelSourceLoc {
    fn default() -> Self {
        RelSourceLoc { offset: 0 }
    }
}

impl fmt::Display for RelSourceLoc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_default() {
            write!(f, "rel_srcloc(0)")
        } else {
            write!(f, "rel_srcloc({})", self.offset)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_srcloc_creation() {
        let srcloc = SourceLoc::new(42);
        assert_eq!(srcloc.bits(), 42);
        assert!(!srcloc.is_default());
    }

    #[test]
    fn test_srcloc_default() {
        let default = SourceLoc::default();
        assert!(default.is_default());
        assert_eq!(default.bits(), !0);
    }

    #[test]
    fn test_rel_srcloc_creation() {
        let rel = RelSourceLoc::new(10);
        assert_eq!(rel.offset, 10);
        assert!(!rel.is_default());
    }

    #[test]
    fn test_rel_srcloc_default() {
        let default = RelSourceLoc::default();
        assert!(default.is_default());
        assert_eq!(default.offset, 0);
    }

    #[test]
    fn test_rel_srcloc_from_base_offset() {
        let base = SourceLoc::new(100);
        let absolute = SourceLoc::new(150);
        let rel = RelSourceLoc::from_base_offset(base, absolute);
        assert_eq!(rel.offset, 50);
    }

    #[test]
    fn test_rel_srcloc_expand() {
        let base = SourceLoc::new(100);
        let rel = RelSourceLoc::new(50);
        let absolute = rel.expand(base);
        assert_eq!(absolute.bits(), 150);
    }

    #[test]
    fn test_rel_srcloc_with_default_base() {
        let base = SourceLoc::default();
        let absolute = SourceLoc::new(100);
        let rel = RelSourceLoc::from_base_offset(base, absolute);
        let expanded = rel.expand(base);
        assert!(expanded.is_default());
    }

    #[test]
    fn test_rel_srcloc_with_default_absolute() {
        let base = SourceLoc::new(100);
        let absolute = SourceLoc::default();
        let rel = RelSourceLoc::from_base_offset(base, absolute);
        // When absolute is default, we use sentinel value, which expands to default
        let expanded = rel.expand(base);
        assert!(expanded.is_default());
    }

    #[test]
    fn test_rel_srcloc_roundtrip() {
        let base = SourceLoc::new(1000);
        let absolute = SourceLoc::new(1050);
        let rel = RelSourceLoc::from_base_offset(base, absolute);
        let expanded = rel.expand(base);
        assert_eq!(expanded.bits(), absolute.bits());
    }
}

