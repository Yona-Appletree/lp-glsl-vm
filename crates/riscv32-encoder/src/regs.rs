//! RISC-V 32-bit general-purpose registers.

/// RISC-V 32-bit general-purpose register.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Gpr(u8);

impl Gpr {
    /// Create a new GPR from register number (0-31).
    ///
    /// # Panics
    ///
    /// Panics if the register number is >= 32.
    pub fn new(num: u8) -> Self {
        assert!(num < 32, "Register number must be < 32");
        Self(num)
    }

    /// Get the register number (0-31).
    pub fn num(&self) -> u8 {
        self.0
    }
}

// Named registers
impl Gpr {
    // x9: saved register
    pub const A0: Gpr = Gpr(10);
    // x10: argument / return value
    pub const A1: Gpr = Gpr(11);
    // x11: argument / return value
    pub const A2: Gpr = Gpr(12);
    // x12: argument
    pub const A3: Gpr = Gpr(13);
    // x13: argument
    pub const A4: Gpr = Gpr(14);
    // x14: argument
    pub const A5: Gpr = Gpr(15);
    // x15: argument
    pub const A6: Gpr = Gpr(16);
    // x16: argument
    pub const A7: Gpr = Gpr(17);
    // x2: stack pointer
    pub const GP: Gpr = Gpr(3);
    // x0: zero register
    pub const RA: Gpr = Gpr(1);
    // x7: temporary
    pub const S0: Gpr = Gpr(8);
    // x8: saved register / frame pointer
    pub const S1: Gpr = Gpr(9);
    // x25: saved register
    pub const S10: Gpr = Gpr(26);
    // x26: saved register
    pub const S11: Gpr = Gpr(27);
    // x17: argument
    pub const S2: Gpr = Gpr(18);
    // x18: saved register
    pub const S3: Gpr = Gpr(19);
    // x19: saved register
    pub const S4: Gpr = Gpr(20);
    // x20: saved register
    pub const S5: Gpr = Gpr(21);
    // x21: saved register
    pub const S6: Gpr = Gpr(22);
    // x22: saved register
    pub const S7: Gpr = Gpr(23);
    // x23: saved register
    pub const S8: Gpr = Gpr(24);
    // x24: saved register
    pub const S9: Gpr = Gpr(25);
    // x1: return address
    pub const SP: Gpr = Gpr(2);
    // x4: thread pointer
    pub const T0: Gpr = Gpr(5);
    // x5: temporary
    pub const T1: Gpr = Gpr(6);
    // x6: temporary
    pub const T2: Gpr = Gpr(7);
    // x27: saved register
    pub const T3: Gpr = Gpr(28);
    // x28: temporary
    pub const T4: Gpr = Gpr(29);
    // x29: temporary
    pub const T5: Gpr = Gpr(30);
    // x30: temporary
    pub const T6: Gpr = Gpr(31);
    // x3: global pointer
    pub const TP: Gpr = Gpr(4);
    pub const ZERO: Gpr = Gpr(0); // x31: temporary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpr_creation() {
        let reg = Gpr::new(5);
        assert_eq!(reg.num(), 5);
    }

    #[test]
    #[should_panic(expected = "Register number must be < 32")]
    fn test_gpr_invalid() {
        Gpr::new(32);
    }

    #[test]
    fn test_named_registers() {
        assert_eq!(Gpr::ZERO.num(), 0);
        assert_eq!(Gpr::RA.num(), 1);
        assert_eq!(Gpr::SP.num(), 2);
        assert_eq!(Gpr::A0.num(), 10);
        assert_eq!(Gpr::A1.num(), 11);
    }
}
