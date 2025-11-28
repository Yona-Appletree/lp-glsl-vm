//! Instruction opcodes.

use alloc::string::String;

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
    /// Integer compare equal: result = (arg1 == arg2)
    IcmpEq,
    /// Integer compare not equal: result = (arg1 != arg2)
    IcmpNe,
    /// Integer compare less than: result = (arg1 < arg2)
    IcmpLt,
    /// Integer compare less than or equal: result = (arg1 <= arg2)
    IcmpLe,
    /// Integer compare greater than: result = (arg1 > arg2)
    IcmpGt,
    /// Integer compare greater than or equal: result = (arg1 >= arg2)
    IcmpGe,

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
