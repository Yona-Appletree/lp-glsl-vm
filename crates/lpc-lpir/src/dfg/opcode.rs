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

    // Bitwise operations
    /// Bitwise AND: result = arg1 & arg2
    Iand,
    /// Bitwise OR: result = arg1 | arg2
    Ior,
    /// Bitwise XOR: result = arg1 ^ arg2
    Ixor,
    /// Bitwise NOT: result = ~arg1 (unary)
    Inot,

    // Shift operations
    /// Left shift: result = arg1 << arg2
    Ishl,
    /// Logical right shift: result = arg1 >>> arg2 (unsigned)
    Ishr,
    /// Arithmetic right shift: result = arg1 >> arg2 (signed)
    Iashr,

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
    /// Stack allocation: result = address of allocated stack space
    StackAlloc {
        /// Size in bytes to allocate
        size: u32,
    },

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

impl Opcode {
    /// Is this a call instruction?
    pub fn is_call(&self) -> bool {
        matches!(self, Opcode::Call { .. })
    }

    /// Is this a terminator (branch, jump, return)?
    pub fn is_terminator(&self) -> bool {
        matches!(
            self,
            Opcode::Jump
                | Opcode::Br
                | Opcode::Return
                | Opcode::Halt
                | Opcode::Trap { .. }
                | Opcode::Trapz { .. }
                | Opcode::Trapnz { .. }
        )
    }

    /// Does this instruction access memory?
    pub fn is_memory_access(&self) -> bool {
        matches!(self, Opcode::Load | Opcode::Store)
    }

    /// Does this instruction have side effects?
    pub fn has_side_effects(&self) -> bool {
        matches!(
            self,
            Opcode::Store
                | Opcode::StackAlloc { .. }
                | Opcode::Call { .. }
                | Opcode::Syscall
                | Opcode::Return
                | Opcode::Trap { .. }
                | Opcode::Trapz { .. }
                | Opcode::Trapnz { .. }
        )
    }
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

    #[test]
    fn test_is_call() {
        assert!(Opcode::Call {
            callee: String::from("foo")
        }
        .is_call());
        assert!(!Opcode::Iadd.is_call());
        assert!(!Opcode::Return.is_call());
        assert!(!Opcode::Jump.is_call());
    }

    #[test]
    fn test_is_terminator() {
        assert!(Opcode::Jump.is_terminator());
        assert!(Opcode::Br.is_terminator());
        assert!(Opcode::Return.is_terminator());
        assert!(Opcode::Halt.is_terminator());
        assert!(Opcode::Trap {
            code: crate::trapcode::TrapCode::STACK_OVERFLOW
        }
        .is_terminator());
        assert!(Opcode::Trapz {
            code: crate::trapcode::TrapCode::STACK_OVERFLOW
        }
        .is_terminator());
        assert!(Opcode::Trapnz {
            code: crate::trapcode::TrapCode::STACK_OVERFLOW
        }
        .is_terminator());

        assert!(!Opcode::Iadd.is_terminator());
        assert!(!Opcode::Call {
            callee: String::from("foo")
        }
        .is_terminator());
        assert!(!Opcode::Load.is_terminator());
    }

    #[test]
    fn test_is_memory_access() {
        assert!(Opcode::Load.is_memory_access());
        assert!(Opcode::Store.is_memory_access());

        assert!(!Opcode::Iadd.is_memory_access());
        assert!(!Opcode::Call {
            callee: String::from("foo")
        }
        .is_memory_access());
        assert!(!Opcode::Return.is_memory_access());
    }

    #[test]
    fn test_has_side_effects() {
        assert!(Opcode::Store.has_side_effects());
        assert!(Opcode::Call {
            callee: String::from("foo")
        }
        .has_side_effects());
        assert!(Opcode::Syscall.has_side_effects());
        assert!(Opcode::Return.has_side_effects());
        assert!(Opcode::Trap {
            code: crate::trapcode::TrapCode::STACK_OVERFLOW
        }
        .has_side_effects());
        assert!(Opcode::Trapz {
            code: crate::trapcode::TrapCode::STACK_OVERFLOW
        }
        .has_side_effects());
        assert!(Opcode::Trapnz {
            code: crate::trapcode::TrapCode::STACK_OVERFLOW
        }
        .has_side_effects());

        assert!(!Opcode::Iadd.has_side_effects());
        assert!(!Opcode::Load.has_side_effects());
        assert!(!Opcode::Iconst.has_side_effects());
        assert!(!Opcode::Icmp {
            cond: crate::condcodes::IntCC::Equal
        }
        .has_side_effects());
    }
}
