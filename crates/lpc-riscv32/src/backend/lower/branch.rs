//! Branch instruction lowering.

use lpc_lpir::Value;
use crate::{Gpr, Inst as RiscvInst};

use super::types::{LoweringError, Relocation, RelocationInstType, RelocationTarget};
use super::super::{emit::CodeBuffer, frame::FrameLayout, regalloc::RegisterAllocation};

impl super::Lowerer {
    /// Lower branch instruction.
    ///
    /// Emits placeholder instructions and records relocations for fixup.
    pub(super) fn lower_br(
        &mut self,
        code: &mut CodeBuffer,
        condition: Value,
        target_true: u32,
        target_false: u32,
        allocation: &RegisterAllocation,
        frame_layout: &FrameLayout,
    ) -> Result<(), LoweringError> {
        // Load condition into a register
        let cond_reg = if let Some(reg) = self.get_register(condition, allocation) {
            reg
        } else {
            let temp = Gpr::T0;
            self.load_value_into_reg(code, condition, temp, allocation, frame_layout)?;
            temp
        };

        // Emit placeholder beq instruction (offset 0, will be fixed up)
        let beq_inst_idx = code.instruction_count();
        code.emit(RiscvInst::Beq {
            rs1: cond_reg,
            rs2: Gpr::Zero,
            imm: 0, // Placeholder
        });

        // Record relocation for beq (false target)
        self.function_relocations.push(Relocation {
            offset: beq_inst_idx,
            target: RelocationTarget::Block(target_false as usize),
            inst_type: RelocationInstType::Beq {
                rs1: cond_reg,
                rs2: Gpr::Zero,
            },
        });

        // Emit placeholder jal instruction (offset 0, will be fixed up)
        let jal_inst_idx = code.instruction_count();
        code.emit(RiscvInst::Jal {
            rd: Gpr::Zero,
            imm: 0, // Placeholder
        });

        // Record relocation for jal (true target)
        self.function_relocations.push(Relocation {
            offset: jal_inst_idx,
            target: RelocationTarget::Block(target_true as usize),
            inst_type: RelocationInstType::Jal { rd: Gpr::Zero },
        });

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use lpc_lpir::parse_function;

    use crate::backend::Lowerer;
    use crate::backend::{
        Abi, FrameLayout, compute_liveness, allocate_registers, create_spill_reload_plan,
    };
    use crate::expect_ir_a0;

    #[test]
    fn test_block_address_recording_and_relocation_fixup() {
        // Test that block addresses are recorded correctly and relocations are fixed up properly
        // This is a simplified version of test_simple_branch_always_true
        let ir = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 42
    v2 = iconst 0
    v3 = iconst 1
    brif v3, block1, block2

block1:
    return v1

block2:
    return v2
}"#;

        let func = parse_function(ir).expect("Failed to parse IR function");
        let liveness = compute_liveness(&func);
        let allocation = allocate_registers(&func, &liveness);
        let spill_reload = create_spill_reload_plan(&func, &allocation, &liveness);

        let has_calls = false;
        let total_spill_slots = allocation.spill_slot_count + spill_reload.max_temp_spill_slots;
        let frame_layout = FrameLayout::compute(
            &allocation.used_callee_saved,
            total_spill_slots,
            has_calls,
            func.signature.params.len(),
            0,
        );

        let abi_info = Abi::compute_abi_info(&func, &allocation, 0);

        let mut lowerer = Lowerer::new();

        // Lower the function
        let code = lowerer
            .lower_function(&func, &allocation, &spill_reload, &frame_layout, &abi_info)
            .expect("Failed to lower function");

        // Check that we have relocations for the branch
        // We expect: 1 beq relocation (false target) + 1 jal relocation (true target) + 2 return relocations (epilogue)
        let expected_relocations = 4; // beq + jal + 2 returns
        assert_eq!(
            lowerer.function_relocations.len(),
            expected_relocations,
            "Expected {} relocations, got {}",
            expected_relocations,
            lowerer.function_relocations.len()
        );

        // Verify that relocations reference valid block indices
        for reloc in &lowerer.function_relocations {
            match &reloc.target {
                crate::backend::lower::RelocationTarget::Block(block_idx) => {
                    assert!(
                        *block_idx < func.blocks.len(),
                        "Relocation references invalid block index {} (function has {} blocks)",
                        block_idx,
                        func.blocks.len()
                    );
                }
                crate::backend::lower::RelocationTarget::Epilogue => {}
                crate::backend::lower::RelocationTarget::Function(_) => {
                    // Function relocations are handled at module level
                }
            }
        }

        // Check that instructions were emitted
        assert!(code.instruction_count().as_usize() > 0, "No instructions were emitted");
    }

    #[test]
    fn test_simple_branch_always_true() {
        // Simplest possible branch: always take true branch
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    v0 = iconst 1
    call %test(v0) -> v1
    halt
}

function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 42
    v2 = iconst 0
    v3 = iconst 1
    brif v3, block1, block2

block1:
    return v1

block2:
    return v2
}
}"#;

        expect_ir_a0(ir, 42);
    }

    #[test]
    fn test_simple_branch_always_false() {
        // Simplest possible branch: always take false branch
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    v0 = iconst 1
    call %test(v0) -> v1
    halt
}

function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 42
    v2 = iconst 0
    v3 = iconst 0
    brif v3, block1, block2

block1:
    return v1

block2:
    return v2
}
}"#;

        expect_ir_a0(ir, 0);
    }

    #[test]
    fn test_simple_fibonacci_base_case() {
        // Test fibonacci base case: if n <= 1, return n
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    v0 = iconst 1
    call %fib(v0) -> v1
    halt
}

function %fib(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 1
    v2 = icmp_le v0, v1
    brif v2, block1, block2

block1:
    return v0

block2:
    v3 = iconst 0
    return v3
}
}"#;

        expect_ir_a0(ir, 1);
    }

    #[test]
    fn test_simple_fibonacci_base_case_zero() {
        // Test fibonacci base case with 0: if n <= 1, return n
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    v0 = iconst 0
    call %fib(v0) -> v1
    halt
}

function %fib(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 1
    v2 = icmp_le v0, v1
    brif v2, block1, block2

block1:
    return v0

block2:
    v3 = iconst 999
    return v3
}
}"#;

        expect_ir_a0(ir, 0);
    }

    #[test]
    fn test_simple_fibonacci_recursive_case() {
        // Test fibonacci recursive case: if n > 1, return 999 (to verify branch works)
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    v0 = iconst 5
    call %fib(v0) -> v1
    halt
}

function %fib(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 1
    v2 = icmp_le v0, v1
    brif v2, block1, block2

block1:
    v3 = iconst 0
    return v3

block2:
    v4 = iconst 999
    return v4
}
}"#;

        expect_ir_a0(ir, 999);
    }

    #[test]
    fn test_nested_branches() {
        // Test nested if/else: if a <= 1, return 1; else if a <= 3, return 2; else return 3
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    v0 = iconst 2
    call %test(v0) -> v1
    halt
}

function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 1
    v2 = iconst 3
    v3 = iconst 1
    v4 = iconst 2
    v5 = iconst 3
    v6 = icmp_le v0, v1
    brif v6, block1, block2

block1:
    return v3

block2:
    v7 = icmp_le v0, v2
    brif v7, block3, block4

block3:
    return v4

block4:
    return v5
}
}"#;

        expect_ir_a0(ir, 2);
    }
}
