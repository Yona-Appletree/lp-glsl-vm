//! Instruction data structure.

use alloc::{string::String, vec::Vec};

use crate::{entity::Block, Type, Value};

/// Instruction data (opcode + operands)
///
/// This structure stores what an instruction does, separate from
/// where it appears in the layout. All instructions follow this
/// uniform structure.
#[derive(Debug, Clone)]
pub struct InstData {
    /// The operation this instruction performs
    pub opcode: crate::dfg::opcode::Opcode,
    /// Input values (arguments)
    pub args: Vec<Value>,
    /// Output values (results, usually 0 or 1)
    pub results: Vec<Value>,
    /// Block arguments for branches/jumps
    pub block_args: Option<BlockArgs>,
    /// Type information (for loads/stores)
    pub ty: Option<Type>,
    /// Immediate values (for constants, syscalls)
    pub imm: Option<Immediate>,
}

/// Block arguments for control flow instructions
///
/// For Jump: single target with args
/// For Br: two targets with args each
#[derive(Debug, Clone)]
pub struct BlockArgs {
    /// Targets with their argument values
    pub targets: Vec<(Block, Vec<Value>)>,
}

/// Immediate values
#[derive(Debug, Clone)]
pub enum Immediate {
    /// 64-bit signed integer
    I64(i64),
    /// 32-bit floating point (stored as bits for Eq compatibility)
    F32Bits(u32),
    /// 32-bit signed integer
    I32(i32),
    /// String (for function names in Call, though Call uses opcode field)
    String(String),
}

impl InstData {
    /// Create a new instruction data with the given opcode
    pub fn new(opcode: crate::dfg::opcode::Opcode) -> Self {
        Self {
            opcode,
            args: Vec::new(),
            results: Vec::new(),
            block_args: None,
            ty: None,
            imm: None,
        }
    }

    /// Create an arithmetic instruction
    pub fn arithmetic(
        opcode: crate::dfg::opcode::Opcode,
        result: Value,
        arg1: Value,
        arg2: Value,
    ) -> Self {
        Self {
            opcode,
            args: Vec::from([arg1, arg2]),
            results: Vec::from([result]),
            block_args: None,
            ty: None,
            imm: None,
        }
    }

    /// Create a comparison instruction
    pub fn comparison(
        opcode: crate::dfg::opcode::Opcode,
        result: Value,
        arg1: Value,
        arg2: Value,
    ) -> Self {
        Self {
            opcode,
            args: Vec::from([arg1, arg2]),
            results: Vec::from([result]),
            block_args: None,
            ty: None,
            imm: None,
        }
    }

    /// Create a constant instruction
    pub fn constant(result: Value, imm: Immediate) -> Self {
        let opcode = match imm {
            Immediate::I64(_) | Immediate::I32(_) => crate::dfg::opcode::Opcode::Iconst,
            Immediate::F32Bits(_) => crate::dfg::opcode::Opcode::Fconst,
            Immediate::String(_) => panic!("String immediate not supported for constants"),
        };
        Self {
            opcode,
            args: Vec::new(),
            results: Vec::from([result]),
            block_args: None,
            ty: None,
            imm: Some(imm),
        }
    }

    /// Create a jump instruction
    pub fn jump(target: Block, args: Vec<Value>) -> Self {
        Self {
            opcode: crate::dfg::opcode::Opcode::Jump,
            args: args.clone(),
            results: Vec::new(),
            block_args: Some(BlockArgs {
                targets: Vec::from([(target, args)]),
            }),
            ty: None,
            imm: None,
        }
    }

    /// Create a branch instruction
    pub fn branch(
        condition: Value,
        target_true: Block,
        args_true: Vec<Value>,
        target_false: Block,
        args_false: Vec<Value>,
    ) -> Self {
        Self {
            opcode: crate::dfg::opcode::Opcode::Br,
            args: {
                let mut all_args = Vec::from([condition]);
                all_args.extend(args_true.iter().copied());
                all_args.extend(args_false.iter().copied());
                all_args
            },
            results: Vec::new(),
            block_args: Some(BlockArgs {
                targets: Vec::from([(target_true, args_true), (target_false, args_false)]),
            }),
            ty: None,
            imm: None,
        }
    }

    /// Create a call instruction
    pub fn call(callee: String, args: Vec<Value>, results: Vec<Value>) -> Self {
        Self {
            opcode: crate::dfg::opcode::Opcode::Call { callee },
            args,
            results,
            block_args: None,
            ty: None,
            imm: None,
        }
    }

    /// Create a syscall instruction
    pub fn syscall(number: i32, args: Vec<Value>) -> Self {
        Self {
            opcode: crate::dfg::opcode::Opcode::Syscall,
            args,
            results: Vec::new(),
            block_args: None,
            ty: None,
            imm: Some(Immediate::I32(number)),
        }
    }

    /// Create a return instruction
    pub fn return_(values: Vec<Value>) -> Self {
        Self {
            opcode: crate::dfg::opcode::Opcode::Return,
            args: values.clone(),
            results: values,
            block_args: None,
            ty: None,
            imm: None,
        }
    }

    /// Create a load instruction
    pub fn load(result: Value, address: Value, ty: Type) -> Self {
        Self {
            opcode: crate::dfg::opcode::Opcode::Load,
            args: Vec::from([address]),
            results: Vec::from([result]),
            block_args: None,
            ty: Some(ty),
            imm: None,
        }
    }

    /// Create a store instruction
    pub fn store(address: Value, value: Value, ty: Type) -> Self {
        Self {
            opcode: crate::dfg::opcode::Opcode::Store,
            args: Vec::from([address, value]),
            results: Vec::new(),
            block_args: None,
            ty: Some(ty),
            imm: None,
        }
    }

    /// Create a halt instruction
    pub fn halt() -> Self {
        Self {
            opcode: crate::dfg::opcode::Opcode::Halt,
            args: Vec::new(),
            results: Vec::new(),
            block_args: None,
            ty: None,
            imm: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use crate::dfg::opcode::Opcode;

    #[test]
    fn test_inst_data_arithmetic() {
        let v1 = Value::new(1);
        let v2 = Value::new(2);
        let v3 = Value::new(3);
        let data = InstData::arithmetic(Opcode::Iadd, v3, v1, v2);

        assert_eq!(data.opcode, Opcode::Iadd);
        assert_eq!(data.args, vec![v1, v2]);
        assert_eq!(data.results, vec![v3]);
    }

    #[test]
    fn test_inst_data_branch() {
        let cond = Value::new(0);
        let v1 = Value::new(1);
        let v2 = Value::new(2);
        let target_true = Block::new(1);
        let target_false = Block::new(2);

        let data = InstData::branch(cond, target_true, vec![v1], target_false, vec![v2]);

        assert_eq!(data.opcode, Opcode::Br);
        assert!(data.block_args.is_some());
        let block_args = data.block_args.as_ref().unwrap();
        assert_eq!(block_args.targets.len(), 2);
        assert_eq!(block_args.targets[0].0, target_true);
        assert_eq!(block_args.targets[0].1, vec![v1]);
        assert_eq!(block_args.targets[1].0, target_false);
        assert_eq!(block_args.targets[1].1, vec![v2]);
    }

    #[test]
    fn test_inst_data_call() {
        let v1 = Value::new(1);
        let v2 = Value::new(2);
        let v3 = Value::new(3);
        let data = InstData::call(String::from("foo"), vec![v1, v2], vec![v3]);

        match data.opcode {
            Opcode::Call { callee } => assert_eq!(callee, "foo"),
            _ => panic!("Expected Call opcode"),
        }
        assert_eq!(data.args, vec![v1, v2]);
        assert_eq!(data.results, vec![v3]);
    }

    #[test]
    fn test_inst_data_load_store() {
        let v1 = Value::new(1);
        let v2 = Value::new(2);
        let load_data = InstData::load(v2, v1, Type::I32);
        let store_data = InstData::store(v1, v2, Type::I32);

        assert_eq!(load_data.opcode, Opcode::Load);
        assert_eq!(load_data.ty, Some(Type::I32));
        assert_eq!(load_data.args, vec![v1]);
        assert_eq!(load_data.results, vec![v2]);

        assert_eq!(store_data.opcode, Opcode::Store);
        assert_eq!(store_data.ty, Some(Type::I32));
        assert_eq!(store_data.args, vec![v1, v2]);
        assert_eq!(store_data.results, Vec::new());
    }

    #[test]
    fn test_inst_data_constant() {
        let v1 = Value::new(1);
        let data = InstData::constant(v1, Immediate::I64(42));

        assert_eq!(data.opcode, Opcode::Iconst);
        assert_eq!(data.results, vec![v1]);
        assert_eq!(data.args, Vec::new());
        match data.imm {
            Some(Immediate::I64(val)) => assert_eq!(val, 42),
            _ => panic!("Expected I64 immediate"),
        }
    }
}
