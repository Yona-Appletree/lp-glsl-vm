//! Functions.

use alloc::{string::String, vec::Vec};
use core::fmt;

use crate::{
    block::BlockData,
    dfg::DFG,
    entity::{Block, Inst},
    entity_map::PrimaryMap,
    layout::Layout,
    signature::Signature,
    sourceloc::{RelSourceLoc, SourceLoc},
};

/// A function in the IR
///
/// A function consists of:
/// - A signature (parameters and return types)
/// - Block data (what blocks are - parameters)
/// - Layout (where blocks/instructions are)
/// - DFG (what instructions are - opcode + operands)
/// - A name (required, for debugging and module lookup)
/// - Source location tracking (for debugging and correlation)
#[derive(Debug, Clone)]
pub struct Function {
    /// Function signature
    pub signature: Signature,
    /// Function name
    pub name: String,
    /// Block data (what blocks are)
    pub blocks: PrimaryMap<Block, BlockData>,
    /// Layout (where blocks/instructions are)
    pub layout: Layout,
    /// Data Flow Graph (what instructions are)
    pub dfg: DFG,
    /// Base source location for this function (used for relative source locations)
    base_srcloc: Option<SourceLoc>,
    /// Relative source locations for instructions (offset from base)
    srclocs: PrimaryMap<Inst, RelSourceLoc>,
}

impl Function {
    /// Create a new function with the given signature and name
    pub fn new(signature: Signature, name: String) -> Self {
        Self {
            signature,
            name,
            blocks: PrimaryMap::new(),
            layout: Layout::new(),
            dfg: DFG::new(),
            base_srcloc: None,
            srclocs: PrimaryMap::new(),
        }
    }

    /// Set the function name
    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }

    /// Get the function name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Create a new block and return its entity
    ///
    /// The block is created with empty parameters. Use `append_block` to add it to the layout.
    pub fn create_block(&mut self) -> Block {
        let block_data = BlockData::new();
        let block = self.blocks.push(block_data);
        self.layout.ensure_block(block);
        block
    }

    /// Create a new block with parameters and return its entity
    ///
    /// The block is created with the given parameters. Use `append_block` to add it to the layout.
    pub fn create_block_with_params(&mut self, params: Vec<crate::value::Value>) -> Block {
        let block_data = BlockData::with_params(params);
        let block = self.blocks.push(block_data);
        self.layout.ensure_block(block);
        block
    }

    /// Create an instruction and return its entity
    ///
    /// The instruction is created in the DFG but not yet inserted into the layout.
    /// Use `append_inst` or `insert_inst` to add it to a block.
    pub fn create_inst(&mut self, data: crate::dfg::InstData) -> Inst {
        let inst = self.dfg.create_inst(data);
        self.layout.ensure_inst(inst);
        inst
    }

    /// Append a block to the end of the layout
    pub fn append_block(&mut self, block: Block) {
        self.layout.append_block(block);
    }

    /// Append an instruction to the end of a block
    pub fn append_inst(&mut self, inst: Inst, block: Block) {
        self.layout.append_inst(inst, block);
    }

    /// Get the entry block (first block in layout order)
    pub fn entry_block(&self) -> Option<Block> {
        self.layout.entry_block()
    }

    /// Get block data
    pub fn block_data(&self, block: Block) -> Option<&BlockData> {
        self.blocks.get(block)
    }

    /// Get mutable block data
    pub fn block_data_mut(&mut self, block: Block) -> Option<&mut BlockData> {
        self.blocks.get_mut(block)
    }

    /// Get the number of blocks in this function
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    /// Get an iterator over blocks in layout order
    pub fn blocks(&self) -> impl Iterator<Item = Block> + '_ {
        self.layout.blocks()
    }

    /// Get an iterator over instructions in a block
    pub fn block_insts(&self, block: Block) -> impl Iterator<Item = Inst> + '_ {
        self.layout.block_insts(block)
    }

    /// Get the base source location for this function.
    ///
    /// Returns the default source location if no base has been set.
    pub fn base_srcloc(&self) -> SourceLoc {
        self.base_srcloc.unwrap_or_default()
    }

    /// Ensure that a base source location is set for this function.
    ///
    /// If a base source location is already set, returns the existing one.
    /// Otherwise, sets the given source location as the base and returns it.
    pub fn ensure_base_srcloc(&mut self, srcloc: SourceLoc) -> SourceLoc {
        if let Some(base) = self.base_srcloc {
            base
        } else {
            self.base_srcloc = Some(srcloc);
            srcloc
        }
    }

    /// Set the absolute source location for an instruction.
    ///
    /// This will automatically set the base source location if not already set,
    /// and store a relative source location for the instruction.
    pub fn set_srcloc(&mut self, inst: Inst, srcloc: SourceLoc) {
        let base = self.ensure_base_srcloc(srcloc);
        let rel = RelSourceLoc::from_base_offset(base, srcloc);
        // Ensure the map is large enough to hold this instruction
        let inst_index = inst.index() as usize;
        while self.srclocs.len() <= inst_index {
            self.srclocs.push(RelSourceLoc::default());
        }
        // Now we can safely set it
        if let Some(existing) = self.srclocs.get_mut(inst) {
            *existing = rel;
        }
    }

    /// Get the absolute source location for an instruction.
    ///
    /// Returns the default source location if the instruction doesn't have
    /// an explicit source location set.
    pub fn srcloc(&self, inst: Inst) -> SourceLoc {
        let base = self.base_srcloc();
        if let Some(rel) = self.srclocs.get(inst) {
            rel.expand(base)
        } else {
            SourceLoc::default()
        }
    }

    /// Format an instruction for display
    fn format_instruction(
        &self,
        f: &mut fmt::Formatter<'_>,
        inst_data: &crate::dfg::InstData,
    ) -> fmt::Result {
        use crate::dfg::{opcode::Opcode, Immediate};

        match &inst_data.opcode {
            // Arithmetic
            Opcode::Iadd | Opcode::Isub | Opcode::Imul | Opcode::Idiv | Opcode::Irem => {
                if inst_data.results.len() == 1 && inst_data.args.len() == 2 {
                    let opname = match inst_data.opcode {
                        Opcode::Iadd => "iadd",
                        Opcode::Isub => "isub",
                        Opcode::Imul => "imul",
                        Opcode::Idiv => "idiv",
                        Opcode::Irem => "irem",
                        _ => unreachable!(),
                    };
                    write!(
                        f,
                        "v{} = {} v{}, v{}",
                        inst_data.results[0].index(),
                        opname,
                        inst_data.args[0].index(),
                        inst_data.args[1].index()
                    )
                } else {
                    write!(f, "{:?}", inst_data.opcode)
                }
            }
            // Integer comparison with condition code
            Opcode::Icmp { cond } => {
                if inst_data.results.len() == 1 && inst_data.args.len() == 2 {
                    write!(
                        f,
                        "v{} = icmp {} v{}, v{}",
                        inst_data.results[0].index(),
                        cond,
                        inst_data.args[0].index(),
                        inst_data.args[1].index()
                    )
                } else {
                    write!(f, "{:?}", inst_data.opcode)
                }
            }
            // Floating point comparison with condition code
            Opcode::Fcmp { cond } => {
                if inst_data.results.len() == 1 && inst_data.args.len() == 2 {
                    write!(
                        f,
                        "v{} = fcmp {} v{}, v{}",
                        inst_data.results[0].index(),
                        cond,
                        inst_data.args[0].index(),
                        inst_data.args[1].index()
                    )
                } else {
                    write!(f, "{:?}", inst_data.opcode)
                }
            }
            // Constants
            Opcode::Iconst | Opcode::Fconst => {
                if inst_data.results.len() == 1 {
                    let opname = match inst_data.opcode {
                        Opcode::Iconst => "iconst",
                        Opcode::Fconst => "fconst",
                        _ => unreachable!(),
                    };
                    if let Some(imm) = &inst_data.imm {
                        match imm {
                            Immediate::I64(val) => {
                                write!(f, "v{} = {} {}", inst_data.results[0].index(), opname, val)
                            }
                            Immediate::I32(val) => {
                                write!(f, "v{} = {} {}", inst_data.results[0].index(), opname, val)
                            }
                            Immediate::F32Bits(bits) => {
                                let val = f32::from_bits(*bits);
                                write!(f, "v{} = {} {}", inst_data.results[0].index(), opname, val)
                            }
                            Immediate::String(_)
                            | Immediate::IntCondCode(_)
                            | Immediate::FloatCondCode(_)
                            | Immediate::TrapCode(_) => write!(f, "{:?}", inst_data.opcode),
                        }
                    } else {
                        write!(f, "{:?}", inst_data.opcode)
                    }
                } else {
                    write!(f, "{:?}", inst_data.opcode)
                }
            }
            // Control flow
            Opcode::Jump => {
                if let Some(block_args) = &inst_data.block_args {
                    if block_args.targets.len() == 1 {
                        let (target, args) = &block_args.targets[0];
                        write!(f, "jump block{}", target.index())?;
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
                    } else {
                        write!(f, "{:?}", inst_data.opcode)
                    }
                } else {
                    write!(f, "{:?}", inst_data.opcode)
                }
            }
            Opcode::Br => {
                if let Some(block_args) = &inst_data.block_args {
                    if block_args.targets.len() == 2 && !inst_data.args.is_empty() {
                        let condition = inst_data.args[0];
                        let (target_true, args_true) = &block_args.targets[0];
                        let (target_false, args_false) = &block_args.targets[1];
                        write!(
                            f,
                            "brif v{}, block{}",
                            condition.index(),
                            target_true.index()
                        )?;
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
                        write!(f, ", block{}", target_false.index())?;
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
                    } else {
                        write!(f, "{:?}", inst_data.opcode)
                    }
                } else {
                    write!(f, "{:?}", inst_data.opcode)
                }
            }
            Opcode::Return => {
                write!(f, "return")?;
                if !inst_data.args.is_empty() {
                    for arg in &inst_data.args {
                        write!(f, " v{}", arg.index())?;
                    }
                }
                Ok(())
            }
            Opcode::Halt => write!(f, "halt"),
            Opcode::Trap { code } => {
                if let Some(Immediate::TrapCode(tc)) = inst_data.imm {
                    write!(f, "trap {}", tc)
                } else {
                    write!(f, "trap {}", code)
                }
            }
            Opcode::Trapz { code } => {
                if inst_data.args.len() == 1 {
                    if let Some(Immediate::TrapCode(tc)) = inst_data.imm {
                        write!(f, "trapz v{}, {}", inst_data.args[0].index(), tc)
                    } else {
                        write!(f, "trapz v{}, {}", inst_data.args[0].index(), code)
                    }
                } else {
                    write!(f, "{:?}", inst_data.opcode)
                }
            }
            Opcode::Trapnz { code } => {
                if inst_data.args.len() == 1 {
                    if let Some(Immediate::TrapCode(tc)) = inst_data.imm {
                        write!(f, "trapnz v{}, {}", inst_data.args[0].index(), tc)
                    } else {
                        write!(f, "trapnz v{}, {}", inst_data.args[0].index(), code)
                    }
                } else {
                    write!(f, "{:?}", inst_data.opcode)
                }
            }
            Opcode::Call { callee } => {
                write!(f, "call %{}(", callee)?;
                for (i, arg) in inst_data.args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "v{}", arg.index())?;
                }
                write!(f, ")")?;
                if !inst_data.results.is_empty() {
                    write!(f, " -> ")?;
                    for (i, res) in inst_data.results.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "v{}", res.index())?;
                    }
                }
                Ok(())
            }
            Opcode::Syscall => {
                if let Some(Immediate::I32(number)) = inst_data.imm {
                    write!(f, "syscall {}(", number)?;
                } else {
                    write!(f, "syscall (")?;
                }
                for (i, arg) in inst_data.args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "v{}", arg.index())?;
                }
                write!(f, ")")
            }
            // Memory
            Opcode::Load => {
                if inst_data.results.len() == 1 && inst_data.args.len() == 1 {
                    if let Some(ty) = inst_data.ty {
                        write!(
                            f,
                            "v{} = load.{} v{}",
                            inst_data.results[0].index(),
                            ty,
                            inst_data.args[0].index()
                        )
                    } else {
                        write!(
                            f,
                            "v{} = load v{}",
                            inst_data.results[0].index(),
                            inst_data.args[0].index()
                        )
                    }
                } else {
                    write!(f, "{:?}", inst_data.opcode)
                }
            }
            Opcode::Store => {
                if inst_data.args.len() == 2 {
                    if let Some(ty) = inst_data.ty {
                        write!(
                            f,
                            "store.{} v{}, v{}",
                            ty,
                            inst_data.args[0].index(),
                            inst_data.args[1].index()
                        )
                    } else {
                        write!(
                            f,
                            "store v{}, v{}",
                            inst_data.args[0].index(),
                            inst_data.args[1].index()
                        )
                    }
                } else {
                    write!(f, "{:?}", inst_data.opcode)
                }
            }
        }
    }
}

impl fmt::Display for Function {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Function name
        write!(f, "function %{}", self.name)?;

        // Signature
        write!(f, "(")?;
        for (i, param_ty) in self.signature.params.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", param_ty)?;
        }
        write!(f, ")")?;

        if !self.signature.returns.is_empty() {
            write!(f, " -> ")?;
            for (i, ret_ty) in self.signature.returns.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{}", ret_ty)?;
            }
        }

        writeln!(f, " {{")?;

        // Print each block with inline parameters (Cranelift format)
        for (block_idx, block) in self.blocks().enumerate() {
            // Format block header with parameters
            write!(f, "block{}", block_idx)?;
            if let Some(block_data) = self.block_data(block) {
                if !block_data.params.is_empty() {
                    write!(f, "(")?;
                    for (i, (param, param_ty)) in block_data
                        .params
                        .iter()
                        .zip(block_data.param_types.iter())
                        .enumerate()
                    {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "v{}: {}", param.index(), param_ty)?;
                    }
                    write!(f, ")")?;
                }
            }
            writeln!(f, ":")?;

            // Print instructions in this block
            for inst in self.block_insts(block) {
                if let Some(inst_data) = self.dfg.inst_data(inst) {
                    write!(f, "    ")?;
                    self.format_instruction(f, inst_data)?;
                    writeln!(f)?;
                }
            }
        }

        writeln!(f, "}}")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use crate::{
        dfg::{InstData, Opcode},
        types::Type,
        value::Value,
    };

    #[test]
    fn test_function_new() {
        let sig = Signature::new(vec![Type::I32, Type::I32], vec![Type::I32]);
        let func = Function::new(sig.clone(), String::from("test"));
        assert_eq!(func.block_count(), 0);
        assert_eq!(func.name(), "test");
    }

    #[test]
    fn test_function_create_block() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let block = func.create_block();
        assert_eq!(func.block_count(), 1);
        assert!(func.block_data(block).is_some());
    }

    #[test]
    fn test_function_create_block_with_params() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let params = vec![Value::new(0), Value::new(1)];
        let block = func.create_block_with_params(params.clone());
        assert_eq!(func.block_count(), 1);
        let block_data = func.block_data(block).unwrap();
        assert_eq!(block_data.params.len(), 2);
    }

    #[test]
    fn test_function_create_inst() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let v1 = Value::new(1);
        let v2 = Value::new(2);
        let v3 = Value::new(3);
        let inst_data = InstData::arithmetic(Opcode::Iadd, v3, v1, v2);
        let inst = func.create_inst(inst_data);
        assert!(func.dfg.inst_data(inst).is_some());
    }

    #[test]
    fn test_function_entry_block() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let block = func.create_block();
        func.append_block(block);
        assert!(func.entry_block().is_some());
        assert_eq!(func.entry_block(), Some(block));
    }

    #[test]
    fn test_function_block_insts() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let block = func.create_block();
        func.append_block(block);

        let v1 = Value::new(1);
        let v2 = Value::new(2);
        let v3 = Value::new(3);
        let inst_data = InstData::arithmetic(Opcode::Iadd, v3, v1, v2);
        let inst = func.create_inst(inst_data);
        func.append_inst(inst, block);

        let insts: Vec<_> = func.block_insts(block).collect();
        assert_eq!(insts.len(), 1);
        assert_eq!(insts[0], inst);
    }

    #[test]
    fn test_function_base_srcloc_default() {
        let sig = Signature::empty();
        let func = Function::new(sig, String::from("test"));
        let base = func.base_srcloc();
        assert!(base.is_default());
    }

    #[test]
    fn test_function_ensure_base_srcloc() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let srcloc1 = SourceLoc::new(100);
        let base1 = func.ensure_base_srcloc(srcloc1);
        assert_eq!(base1.bits(), 100);
        assert_eq!(func.base_srcloc().bits(), 100);

        // Setting again should return the existing base
        let srcloc2 = SourceLoc::new(200);
        let base2 = func.ensure_base_srcloc(srcloc2);
        assert_eq!(base2.bits(), 100); // Should still be the first one
        assert_eq!(func.base_srcloc().bits(), 100);
    }

    #[test]
    fn test_function_set_and_get_srcloc() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let block = func.create_block();
        func.append_block(block);

        let v1 = Value::new(1);
        let v2 = Value::new(2);
        let v3 = Value::new(3);
        let inst_data = InstData::arithmetic(Opcode::Iadd, v3, v1, v2);
        let inst = func.create_inst(inst_data);

        // Initially, instruction should have default source location
        let default_srcloc = func.srcloc(inst);
        assert!(default_srcloc.is_default());

        // Set a source location
        let srcloc = SourceLoc::new(150);
        func.set_srcloc(inst, srcloc);

        // Retrieve it
        let retrieved = func.srcloc(inst);
        assert_eq!(retrieved.bits(), srcloc.bits());
        assert!(!retrieved.is_default());
    }

    #[test]
    fn test_function_srcloc_without_base() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let block = func.create_block();
        func.append_block(block);

        let v1 = Value::new(1);
        let v2 = Value::new(2);
        let v3 = Value::new(3);
        let inst_data = InstData::arithmetic(Opcode::Iadd, v3, v1, v2);
        let inst = func.create_inst(inst_data);

        // Set a source location - this should automatically set the base
        let srcloc = SourceLoc::new(200);
        func.set_srcloc(inst, srcloc);

        // Base should now be set
        assert_eq!(func.base_srcloc().bits(), 200);
        assert_eq!(func.srcloc(inst).bits(), 200);
    }

    #[test]
    fn test_function_srcloc_relative() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let block = func.create_block();
        func.append_block(block);

        // Set base source location
        let base = SourceLoc::new(1000);
        func.ensure_base_srcloc(base);

        let v1 = Value::new(1);
        let v2 = Value::new(2);
        let v3 = Value::new(3);
        let inst_data = InstData::arithmetic(Opcode::Iadd, v3, v1, v2);
        let inst = func.create_inst(inst_data);

        // Set a source location relative to base
        let srcloc = SourceLoc::new(1050);
        func.set_srcloc(inst, srcloc);

        // Retrieve it - should expand correctly
        let retrieved = func.srcloc(inst);
        assert_eq!(retrieved.bits(), 1050);
    }

    #[test]
    fn test_function_srcloc_multiple_instructions() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let block = func.create_block();
        func.append_block(block);

        let base = SourceLoc::new(500);
        func.ensure_base_srcloc(base);

        let v1 = Value::new(1);
        let v2 = Value::new(2);
        let v3 = Value::new(3);
        let v4 = Value::new(4);

        // Create two instructions
        let inst1_data = InstData::arithmetic(Opcode::Iadd, v3, v1, v2);
        let inst1 = func.create_inst(inst1_data);
        let inst2_data = InstData::arithmetic(Opcode::Isub, v4, v3, v1);
        let inst2 = func.create_inst(inst2_data);

        // Set different source locations
        func.set_srcloc(inst1, SourceLoc::new(600));
        func.set_srcloc(inst2, SourceLoc::new(700));

        // Verify both are stored correctly
        assert_eq!(func.srcloc(inst1).bits(), 600);
        assert_eq!(func.srcloc(inst2).bits(), 700);
    }
}
