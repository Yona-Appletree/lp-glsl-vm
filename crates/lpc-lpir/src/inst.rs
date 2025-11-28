//! IR instructions.

use alloc::{string::String, vec, vec::Vec};
use core::fmt;

use crate::{
    condcodes::{FloatCC, IntCC},
    trapcode::TrapCode,
    types::Type,
    value::Value,
};

/// An IR instruction.
#[derive(Debug, Clone)]
pub enum Inst {
    // Arithmetic
    /// Integer add: result = arg1 + arg2
    Iadd {
        result: Value,
        arg1: Value,
        arg2: Value,
    },
    /// Integer subtract: result = arg1 - arg2
    Isub {
        result: Value,
        arg1: Value,
        arg2: Value,
    },
    /// Integer multiply: result = arg1 * arg2
    Imul {
        result: Value,
        arg1: Value,
        arg2: Value,
    },
    /// Integer divide: result = arg1 / arg2
    Idiv {
        result: Value,
        arg1: Value,
        arg2: Value,
    },
    /// Integer remainder: result = arg1 % arg2
    Irem {
        result: Value,
        arg1: Value,
        arg2: Value,
    },

    // Comparisons
    /// Integer comparison: result = (arg1 cond arg2)
    Icmp {
        result: Value,
        cond: IntCC,
        arg1: Value,
        arg2: Value,
    },
    /// Floating point comparison: result = (arg1 cond arg2)
    /// Note: IR-only, backend lowering not supported yet
    Fcmp {
        result: Value,
        cond: FloatCC,
        arg1: Value,
        arg2: Value,
    },

    // Constants
    /// Integer constant: result = value
    Iconst { result: Value, value: i64 },
    /// Floating point constant: result = value
    /// Note: Uses u32 to represent f32 bits for Eq compatibility
    Fconst { result: Value, value_bits: u32 },

    // Control flow
    /// Jump to block
    Jump {
        target: u32,
        /// Values passed to target block parameters
        args: Vec<Value>,
    },
    /// Conditional branch: if condition, jump to target_true, else target_false
    Br {
        condition: Value,
        target_true: u32,
        /// Values passed to target_true block parameters
        args_true: Vec<Value>,
        target_false: u32,
        /// Values passed to target_false block parameters
        args_false: Vec<Value>,
    },
    /// Function call: results = callee(args...)
    Call {
        /// Function name to call (must exist in module)
        callee: String,
        /// Argument values
        args: Vec<Value>,
        /// Result values (SSA)
        results: Vec<Value>,
    },
    /// System call: syscall(number, args...)
    Syscall {
        /// Syscall number
        number: i32,
        /// Argument values (passed in a0-a7)
        args: Vec<Value>,
    },
    /// Return with values
    Return { values: Vec<Value> },
    /// Halt execution (ebreak)
    Halt,

    // Memory
    /// Load from memory: result = mem[address]
    Load {
        result: Value,
        address: Value,
        ty: Type,
    },
    /// Store to memory: mem[address] = value
    Store {
        address: Value,
        value: Value,
        ty: Type,
    },

    // Traps
    /// Unconditional trap: terminate execution with trap code
    Trap { code: TrapCode },
    /// Trap if condition is zero: if condition == 0, trap with code
    Trapz { condition: Value, code: TrapCode },
    /// Trap if condition is non-zero: if condition != 0, trap with code
    Trapnz { condition: Value, code: TrapCode },
}

impl Inst {
    /// Get the result value(s) produced by this instruction, if any.
    pub fn results(&self) -> Vec<Value> {
        match self {
            Inst::Iadd { result, .. }
            | Inst::Isub { result, .. }
            | Inst::Imul { result, .. }
            | Inst::Idiv { result, .. }
            | Inst::Irem { result, .. }
            | Inst::Icmp { result, .. }
            | Inst::Fcmp { result, .. }
            | Inst::Iconst { result, .. }
            | Inst::Fconst { result, .. }
            | Inst::Load { result, .. } => vec![*result],
            Inst::Call { results, .. } => results.clone(),
            Inst::Syscall { .. } => Vec::new(),
            Inst::Jump { .. }
            | Inst::Br { .. }
            | Inst::Store { .. }
            | Inst::Halt
            | Inst::Trap { .. }
            | Inst::Trapz { .. }
            | Inst::Trapnz { .. } => Vec::new(),
            Inst::Return { values } => values.clone(),
        }
    }

    /// Get the argument values used by this instruction.
    pub fn args(&self) -> Vec<Value> {
        match self {
            Inst::Iadd { arg1, arg2, .. }
            | Inst::Isub { arg1, arg2, .. }
            | Inst::Imul { arg1, arg2, .. }
            | Inst::Idiv { arg1, arg2, .. }
            | Inst::Irem { arg1, arg2, .. }
            | Inst::Icmp { arg1, arg2, .. }
            | Inst::Fcmp { arg1, arg2, .. } => vec![*arg1, *arg2],
            Inst::Iconst { .. } | Inst::Fconst { .. } | Inst::Halt | Inst::Trap { .. } => {
                Vec::new()
            }
            Inst::Trapz { condition, .. } | Inst::Trapnz { condition, .. } => vec![*condition],
            Inst::Jump { args, .. } => args.clone(),
            Inst::Br {
                condition,
                args_true,
                args_false,
                ..
            } => {
                let mut all_args = vec![*condition];
                all_args.extend(args_true);
                all_args.extend(args_false);
                all_args
            }
            Inst::Call { args, .. } => args.clone(),
            Inst::Syscall { args, .. } => args.clone(),
            Inst::Return { values } => values.clone(),
            Inst::Load { address, .. } => vec![*address],
            Inst::Store { address, value, .. } => vec![*address, *value],
        }
    }
}

impl fmt::Display for Inst {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Inst::Iadd { result, arg1, arg2 } => {
                write!(
                    f,
                    "v{} = iadd v{}, v{}",
                    result.index(),
                    arg1.index(),
                    arg2.index()
                )
            }
            Inst::Isub { result, arg1, arg2 } => {
                write!(
                    f,
                    "v{} = isub v{}, v{}",
                    result.index(),
                    arg1.index(),
                    arg2.index()
                )
            }
            Inst::Imul { result, arg1, arg2 } => {
                write!(
                    f,
                    "v{} = imul v{}, v{}",
                    result.index(),
                    arg1.index(),
                    arg2.index()
                )
            }
            Inst::Idiv { result, arg1, arg2 } => {
                write!(
                    f,
                    "v{} = idiv v{}, v{}",
                    result.index(),
                    arg1.index(),
                    arg2.index()
                )
            }
            Inst::Irem { result, arg1, arg2 } => {
                write!(
                    f,
                    "v{} = irem v{}, v{}",
                    result.index(),
                    arg1.index(),
                    arg2.index()
                )
            }
            Inst::Icmp {
                result,
                cond,
                arg1,
                arg2,
            } => {
                write!(
                    f,
                    "v{} = icmp {} v{}, v{}",
                    result.index(),
                    cond,
                    arg1.index(),
                    arg2.index()
                )
            }
            Inst::Fcmp {
                result,
                cond,
                arg1,
                arg2,
            } => {
                write!(
                    f,
                    "v{} = fcmp {} v{}, v{}",
                    result.index(),
                    cond,
                    arg1.index(),
                    arg2.index()
                )
            }
            Inst::Iconst { result, value } => {
                write!(f, "v{} = iconst {}", result.index(), value)
            }
            Inst::Fconst { result, value_bits } => {
                // Decode f32 from bits
                let f32_value = f32::from_bits(*value_bits);
                write!(f, "v{} = fconst {}", result.index(), f32_value)
            }
            Inst::Jump { target, args } => {
                write!(f, "jump block{}", target)?;
                if !args.is_empty() {
                    write!(f, "(")?;
                    for (i, arg) in args.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "v{}", arg.index())?;
                    }
                    write!(f, ")")?;
                }
                Ok(())
            }
            Inst::Br {
                condition,
                target_true,
                args_true,
                target_false,
                args_false,
            } => {
                write!(f, "brif v{}, block{}", condition.index(), target_true)?;
                if !args_true.is_empty() {
                    write!(f, "(")?;
                    for (i, arg) in args_true.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "v{}", arg.index())?;
                    }
                    write!(f, ")")?;
                }
                write!(f, ", block{}", target_false)?;
                if !args_false.is_empty() {
                    write!(f, "(")?;
                    for (i, arg) in args_false.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "v{}", arg.index())?;
                    }
                    write!(f, ")")?;
                }
                Ok(())
            }
            Inst::Call {
                callee,
                args,
                results,
            } => {
                write!(f, "call %{}(", callee)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "v{}", arg.index())?;
                }
                write!(f, ")")?;
                if !results.is_empty() {
                    write!(f, " -> ")?;
                    for (i, res) in results.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "v{}", res.index())?;
                    }
                }
                Ok(())
            }
            Inst::Syscall { number, args } => {
                write!(f, "syscall {}(", number)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "v{}", arg.index())?;
                }
                write!(f, ")")
            }
            Inst::Return { values } => {
                write!(f, "return")?;
                if !values.is_empty() {
                    for val in values.iter() {
                        write!(f, " v{}", val.index())?;
                    }
                }
                Ok(())
            }
            Inst::Load {
                result,
                address,
                ty,
            } => {
                write!(f, "v{} = load.{} v{}", result.index(), ty, address.index())
            }
            Inst::Store { address, value, ty } => {
                write!(f, "store.{} v{}, v{}", ty, address.index(), value.index())
            }
            Inst::Halt => write!(f, "halt"),
            Inst::Trap { code } => {
                write!(f, "trap {}", code)
            }
            Inst::Trapz { condition, code } => {
                write!(f, "trapz v{}, {}", condition.index(), code)
            }
            Inst::Trapnz { condition, code } => {
                write!(f, "trapnz v{}, {}", condition.index(), code)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    #[test]
    fn test_iadd() {
        let v1 = Value::new(1);
        let v2 = Value::new(2);
        let v3 = Value::new(3);
        let inst = Inst::Iadd {
            result: v3,
            arg1: v1,
            arg2: v2,
        };
        // Note: Can't compare Inst directly due to f32 in Fconst, but we can test methods
        let results = inst.results();
        let args = inst.args();
        assert_eq!(results.len(), 1);
        assert_eq!(args.len(), 2);
    }

    #[test]
    fn test_iconst() {
        let v1 = Value::new(1);
        let inst = Inst::Iconst {
            result: v1,
            value: 42,
        };
        assert_eq!(inst.results(), vec![v1]);
        assert_eq!(inst.args(), Vec::new());
    }

    #[test]
    fn test_return() {
        let v1 = Value::new(1);
        let v2 = Value::new(2);
        let inst = Inst::Return {
            values: vec![v1, v2],
        };
        assert_eq!(inst.results(), vec![v1, v2]);
        assert_eq!(inst.args(), vec![v1, v2]);
    }
}
