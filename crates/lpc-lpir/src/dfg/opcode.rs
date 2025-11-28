//! Instruction opcodes.

use alloc::string::String;

use crate::{
    condcodes::{FloatCC, IntCC},
    trapcode::TrapCode,
};

/// Instruction opcode
///
/// This enum represents the operation that an instruction performs.
/// It's separate from the instruction data (operands, results, etc.)
/// which are stored in InstData.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Opcode {
    // Arithmetic
    /// Integer add: result = arg1 + arg2
    Iadd,
    /// Integer subtract: result = arg1 - arg2
    Isub,
    /// Integer multiply: result = arg1 * arg2
    Imul,
    /// Integer divide: result = arg1 / arg2
    Idiv,
    /// Integer remainder: result = arg1 % arg2
    Irem,

    // Comparisons
    /// Integer comparison with condition code: result = (arg1 cond arg2)
    Icmp {
        /// Condition code for the comparison
        cond: IntCC,
    },
    /// Floating point comparison with condition code: result = (arg1 cond arg2)
    /// Note: IR-only, backend lowering not supported yet
    Fcmp {
        /// Condition code for the comparison
        cond: FloatCC,
    },

    // Constants
    /// Integer constant: result = value
    Iconst,
    /// Floating point constant: result = value
    Fconst,

    // Control flow
    /// Jump to block
    Jump,
    /// Conditional branch: if condition, jump to target_true, else target_false
    Br,
    /// Return with values
    Return,
    /// Function call: results = callee(args...)
    Call {
        /// Function name to call (must exist in module)
        callee: String,
    },
    /// System call: syscall(number, args...)
    Syscall,
    /// Halt execution (ebreak)
    Halt,

    // Memory
    /// Load from memory: result = mem[address]
    Load,
    /// Store to memory: mem[address] = value
    Store,

    // Traps
    /// Unconditional trap: terminate execution with trap code
    Trap {
        /// Trap code describing the reason for the trap
        code: TrapCode,
    },
    /// Trap if condition is zero: if condition == 0, trap with code
    Trapz {
        /// Trap code describing the reason for the trap
        code: TrapCode,
    },
    /// Trap if condition is non-zero: if condition != 0, trap with code
    Trapnz {
        /// Trap code describing the reason for the trap
        code: TrapCode,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opcode_equality() {
        assert_eq!(Opcode::Iadd, Opcode::Iadd);
        assert_eq!(Opcode::Isub, Opcode::Isub);
        assert_ne!(Opcode::Iadd, Opcode::Isub);
    }

    #[test]
    fn test_opcode_call_with_name() {
        let call1 = Opcode::Call {
            callee: String::from("foo"),
        };
        let call2 = Opcode::Call {
            callee: String::from("foo"),
        };
        let call3 = Opcode::Call {
            callee: String::from("bar"),
        };

        assert_eq!(call1, call2);
        assert_ne!(call1, call3);
    }
}
