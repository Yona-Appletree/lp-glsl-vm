//! RISC-V 32-bit general-purpose registers.

extern crate alloc;

use core::fmt;

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

    /// Parse a register name string into a Gpr.
    ///
    /// Supports both named registers (zero, ra, sp, a0-a7, s0-s11, t0-t6, etc.)
    /// and numeric registers (x0-x31).
    ///
    /// # Errors
    ///
    /// Returns an error string if the register name is invalid.
    pub fn from_name(name: &str) -> Result<Self, alloc::string::String> {
        match name {
            "zero" | "x0" => Ok(Gpr::ZERO),
            "ra" | "x1" => Ok(Gpr::RA),
            "sp" | "x2" => Ok(Gpr::SP),
            "gp" | "x3" => Ok(Gpr::GP),
            "tp" | "x4" => Ok(Gpr::TP),
            "t0" | "x5" => Ok(Gpr::T0),
            "t1" | "x6" => Ok(Gpr::T1),
            "t2" | "x7" => Ok(Gpr::T2),
            "s0" | "fp" | "x8" => Ok(Gpr::S0),
            "s1" | "x9" => Ok(Gpr::S1),
            "a0" | "x10" => Ok(Gpr::A0),
            "a1" | "x11" => Ok(Gpr::A1),
            "a2" | "x12" => Ok(Gpr::A2),
            "a3" | "x13" => Ok(Gpr::A3),
            "a4" | "x14" => Ok(Gpr::A4),
            "a5" | "x15" => Ok(Gpr::A5),
            "a6" | "x16" => Ok(Gpr::A6),
            "a7" | "x17" => Ok(Gpr::A7),
            "s2" | "x18" => Ok(Gpr::S2),
            "s3" | "x19" => Ok(Gpr::S3),
            "s4" | "x20" => Ok(Gpr::S4),
            "s5" | "x21" => Ok(Gpr::S5),
            "s6" | "x22" => Ok(Gpr::S6),
            "s7" | "x23" => Ok(Gpr::S7),
            "s8" | "x24" => Ok(Gpr::S8),
            "s9" | "x25" => Ok(Gpr::S9),
            "s10" | "x26" => Ok(Gpr::S10),
            "s11" | "x27" => Ok(Gpr::S11),
            "t3" | "x28" => Ok(Gpr::T3),
            "t4" | "x29" => Ok(Gpr::T4),
            "t5" | "x30" => Ok(Gpr::T5),
            "t6" | "x31" => Ok(Gpr::T6),
            _ => {
                // Try parsing as xN or numeric
                if let Some(num_str) = name.strip_prefix("x") {
                    if let Ok(num) = num_str.parse::<u8>() {
                        if num < 32 {
                            return Ok(Gpr::new(num));
                        }
                    }
                }
                Err(alloc::format!("Invalid register name: {}", name))
            }
        }
    }
}

impl fmt::Display for Gpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self.0 {
            0 => "zero",
            1 => "ra",
            2 => "sp",
            3 => "gp",
            4 => "tp",
            5 => "t0",
            6 => "t1",
            7 => "t2",
            8 => "s0",
            9 => "s1",
            10 => "a0",
            11 => "a1",
            12 => "a2",
            13 => "a3",
            14 => "a4",
            15 => "a5",
            16 => "a6",
            17 => "a7",
            18 => "s2",
            19 => "s3",
            20 => "s4",
            21 => "s5",
            22 => "s6",
            23 => "s7",
            24 => "s8",
            25 => "s9",
            26 => "s10",
            27 => "s11",
            28 => "t3",
            29 => "t4",
            30 => "t5",
            31 => "t6",
            _ => unreachable!(),
        };
        write!(f, "{}", name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;

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

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", Gpr::ZERO), "zero");
        assert_eq!(format!("{}", Gpr::RA), "ra");
        assert_eq!(format!("{}", Gpr::SP), "sp");
        assert_eq!(format!("{}", Gpr::GP), "gp");
        assert_eq!(format!("{}", Gpr::TP), "tp");
        assert_eq!(format!("{}", Gpr::T0), "t0");
        assert_eq!(format!("{}", Gpr::S0), "s0");
        assert_eq!(format!("{}", Gpr::A0), "a0");
        assert_eq!(format!("{}", Gpr::A1), "a1");
        assert_eq!(format!("{}", Gpr::T6), "t6");
    }
}
