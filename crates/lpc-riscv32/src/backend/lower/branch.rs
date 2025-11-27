//! Lower control flow instructions.

use super::{BranchType, Lowerer, Relocation};
use crate::Gpr;

/// Lower JUMP: jump to target block
pub fn lower_jump(lowerer: &mut Lowerer, target: u32) {
    let inst_idx = lowerer.inst_buffer_mut().instruction_count();

    // Emit JAL with placeholder offset (0)
    lowerer.inst_buffer_mut().emit(crate::Inst::Jal {
        rd: Gpr::Zero,
        imm: 0, // Placeholder, will be fixed up
    });

    // Record relocation
    lowerer.record_relocation(Relocation {
        inst_idx,
        target_block: target,
        branch_type: BranchType::Jump,
    });
}

/// Lower BR: if condition, jump to target_true, else target_false
pub fn lower_br(
    lowerer: &mut Lowerer,
    condition: lpc_lpir::Value,
    target_true: u32,
    target_false: u32,
) {
    let condition_reg = lowerer.get_reg_for_value_required(condition);

    // Emit conditional branch for true target
    // Compare condition with zero: if condition != 0, jump to target_true
    // Use BNE: if condition_reg != zero, jump to target_true
    let branch_inst_idx = lowerer.inst_buffer_mut().instruction_count();
    lowerer.inst_buffer_mut().emit(crate::Inst::Bne {
        rs1: condition_reg,
        rs2: Gpr::Zero,
        imm: 0, // Placeholder
    });

    // Record relocation for true target
    lowerer.record_relocation(Relocation {
        inst_idx: branch_inst_idx,
        target_block: target_true,
        branch_type: BranchType::BranchTrue,
    });

    // Handle false target
    // For now, always emit a jump (we can optimize fall-through later)
    let jump_inst_idx = lowerer.inst_buffer_mut().instruction_count();
    lowerer.inst_buffer_mut().emit(crate::Inst::Jal {
        rd: Gpr::Zero,
        imm: 0, // Placeholder
    });

    // Record relocation for false target
    lowerer.record_relocation(Relocation {
        inst_idx: jump_inst_idx,
        target_block: target_false,
        branch_type: BranchType::BranchFalse,
    });
}

#[cfg(test)]
mod tests {
    extern crate alloc;
    use lpc_lpir::{Block, Function, Signature, Type};

    use super::*;
    use crate::Gpr;

    fn create_test_function() -> Function {
        let sig = Signature::new(alloc::vec![Type::I32], alloc::vec![Type::I32]);
        let mut func = Function::new(sig);
        let block0 = Block::new();
        func.blocks.push(block0);
        func
    }

    #[test]
    fn test_lower_jump_records_relocation() {
        let func = create_test_function();
        let mut lowerer = Lowerer::new(func);

        // Lower a jump instruction
        lower_jump(&mut lowerer, 0);

        // Check that relocation was recorded
        assert_eq!(lowerer.relocations().len(), 1);
        let reloc = &lowerer.relocations()[0];
        assert_eq!(reloc.target_block, 0);
        assert!(matches!(reloc.branch_type, BranchType::Jump));

        // Check that JAL was emitted with placeholder offset
        lowerer.inst_buffer.assert_asm("jal zero, 0");
    }

    #[test]
    fn test_lower_br_records_relocations() {
        let mut func = create_test_function();
        let block0 = &mut func.blocks[0];

        // Add a constant value for the condition
        let v0 = lpc_lpir::Value::new(0);
        block0.insts.push(lpc_lpir::Inst::Iconst {
            result: v0,
            value: 1,
        });

        let mut lowerer = Lowerer::new(func);

        // Allocate register for condition value
        lowerer.get_reg_for_value(v0);

        // Lower a branch instruction
        lower_br(&mut lowerer, v0, 0, 1);

        // Check that two relocations were recorded (true and false targets)
        assert_eq!(lowerer.relocations().len(), 2);

        let reloc_true = &lowerer.relocations()[0];
        assert_eq!(reloc_true.target_block, 0);
        assert!(matches!(reloc_true.branch_type, BranchType::BranchTrue));

        let reloc_false = &lowerer.relocations()[1];
        assert_eq!(reloc_false.target_block, 1);
        assert!(matches!(reloc_false.branch_type, BranchType::BranchFalse));

        // Check that BNE and JAL were emitted with placeholder offsets
        // Note: We can't check the exact register for BNE since it depends on register allocation
        // But we can verify the instruction sequence
        let insts = lowerer.inst_buffer.instructions();
        assert_eq!(insts.len(), 2);
        assert!(matches!(insts[0], crate::Inst::Bne { imm: 0, .. }));
        assert!(matches!(
            insts[1],
            crate::Inst::Jal {
                rd: Gpr::Zero,
                imm: 0
            }
        ));
    }
}

#[cfg(test)]
mod fixup_tests {
    extern crate alloc;
    use lpc_lpir::{Block, Function, Signature, Type};

    use super::*;
    use crate::Gpr;

    fn create_test_function() -> Function {
        let sig = Signature::new(alloc::vec![Type::I32], alloc::vec![Type::I32]);
        let mut func = Function::new(sig);
        func.blocks.push(Block::new());
        func.blocks.push(Block::new());
        func
    }

    #[test]
    fn test_fixup_forward_jump() {
        let func = create_test_function();
        let mut lowerer = Lowerer::new(func);

        // Initialize block addresses
        lowerer.block_addresses.resize(2, 0);
        lowerer.block_addresses[0] = 0; // Block 0 starts at instruction 0
        lowerer.block_addresses[1] = 5; // Block 1 starts at instruction 5

        // Emit some instructions
        for _ in 0..2 {
            lowerer.inst_buffer_mut().emit(crate::Inst::Addi {
                rd: Gpr::A0,
                rs1: Gpr::Zero,
                imm: 0,
            });
        }
        // Instruction 2: the jump (will be fixed up)
        lowerer.inst_buffer_mut().emit(crate::Inst::Jal {
            rd: Gpr::Zero,
            imm: 0, // Placeholder
        });

        // Add a relocation: jump from instruction 2 to block 1
        lowerer.record_relocation(Relocation {
            inst_idx: 2,
            target_block: 1,
            branch_type: BranchType::Jump,
        });

        // Fix up relocations
        lowerer.fixup_relocations();

        // Check that the jump instruction was fixed up
        // Instruction 2 should now have offset = 5 - 2 = 3
        lowerer.inst_buffer.assert_asm(
            "
            addi a0, zero, 0
            addi a0, zero, 0
            jal zero, 3
        ",
        );
    }

    #[test]
    fn test_fixup_backward_jump() {
        let func = create_test_function();
        let mut lowerer = Lowerer::new(func);

        lowerer.block_addresses.resize(2, 0);
        lowerer.block_addresses[0] = 0;
        lowerer.block_addresses[1] = 10;

        // Emit instructions
        for _ in 0..16 {
            lowerer.inst_buffer_mut().emit(crate::Inst::Addi {
                rd: Gpr::A0,
                rs1: Gpr::Zero,
                imm: 0,
            });
        }
        // Replace instruction 15 with a jump
        lowerer.inst_buffer_mut().set_instruction(
            15,
            crate::Inst::Jal {
                rd: Gpr::Zero,
                imm: 0,
            },
        );

        // Add a relocation: jump from instruction 15 (in block 1) back to block 0
        lowerer.record_relocation(Relocation {
            inst_idx: 15,
            target_block: 0,
            branch_type: BranchType::Jump,
        });

        // Fix up relocations
        lowerer.fixup_relocations();

        // Check that the jump instruction was fixed up with negative offset
        // Offset = 0 - 15 = -15
        // We need to check instruction 15 specifically, so we'll verify the offset directly
        let insts = lowerer.inst_buffer.instructions();
        assert_eq!(insts.len(), 16);
        assert!(matches!(
            insts[15],
            crate::Inst::Jal {
                rd: Gpr::Zero,
                imm: -15
            }
        ));
    }

    #[test]
    fn test_fixup_conditional_branch() {
        let mut func = create_test_function();
        let block0 = &mut func.blocks[0];

        let v0 = lpc_lpir::Value::new(0);
        block0.insts.push(lpc_lpir::Inst::Iconst {
            result: v0,
            value: 1,
        });

        let mut lowerer = Lowerer::new(func);
        lowerer.get_reg_for_value(v0);

        lowerer.block_addresses.resize(2, 0);
        lowerer.block_addresses[0] = 0;
        lowerer.block_addresses[1] = 10;

        // Lower a branch
        lower_br(&mut lowerer, v0, 1, 0);

        // Fix up relocations
        lowerer.fixup_relocations();

        // Check that both branches were fixed up
        let insts = lowerer.inst_buffer.instructions();
        assert_eq!(insts.len(), 2);

        // BNE should have offset = 10 - 0 = 10
        assert!(matches!(insts[0], crate::Inst::Bne { imm: 10, .. }));

        // JAL should have offset = 0 - 1 = -1
        assert!(matches!(
            insts[1],
            crate::Inst::Jal {
                rd: Gpr::Zero,
                imm: -1
            }
        ));
    }
}
