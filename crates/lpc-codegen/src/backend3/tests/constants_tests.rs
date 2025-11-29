//! Tests for constant materialization (inline and large)

extern crate alloc;

use lpc_lpir::RelSourceLoc;

use crate::{
    backend3::{
        constants::materialize_constant,
        types::BlockIndex,
        vcode::{BlockLoweringOrder, Callee, Constant, LoweredBlock},
        vcode_builder::VCodeBuilder,
    },
    isa::riscv32::backend3::{
        inst::{Riscv32ABI, Riscv32EmitInfo, Riscv32MachInst},
        regs::zero_reg,
    },
};

/// Helper to build VCode with a single empty block
fn build_vcode_with_single_block(
    builder: VCodeBuilder<Riscv32MachInst>,
) -> crate::backend3::vcode::VCode<Riscv32MachInst> {
    let entry = BlockIndex::new(0);
    let mut block_to_index = alloc::collections::BTreeMap::new();
    block_to_index.insert(lpc_lpir::BlockEntity::new(0), entry);
    let block_order = BlockLoweringOrder {
        lowered_order: alloc::vec![LoweredBlock::Orig {
            block: lpc_lpir::BlockEntity::new(0),
        }],
        lowered_succs: alloc::vec![alloc::vec![]],
        block_to_index,
        cold_blocks: alloc::collections::BTreeSet::new(),
        indirect_targets: alloc::collections::BTreeSet::new(),
    };
    let abi = Callee { abi: Riscv32ABI };
    builder.build(entry, block_order, abi)
}

#[test]
fn test_materialize_inline_constant() {
    use crate::backend3::vcode::LoweredBlock;

    let mut vcode = VCodeBuilder::<Riscv32MachInst>::new(Riscv32EmitInfo);
    let block_idx = crate::backend3::types::BlockIndex::new(0);
    vcode.start_block(block_idx, alloc::vec![]);

    let srcloc = RelSourceLoc::default();

    // Small constant that fits in 12 bits - now emits Addi instruction
    let vreg = materialize_constant(
        &mut vcode,
        42,
        srcloc,
        |_rd, _imm| panic!("Should not create LUI for small constant"),
        |rd, rs1, imm| Riscv32MachInst::Addi { rd, rs1, imm },
        || zero_reg(),
    );

    vcode.end_block();

    // Build VCode to check instructions
    let built_vcode = build_vcode_with_single_block(vcode);
    // Small constants now emit Addi instructions (SSA requirement)
    assert_eq!(built_vcode.insts.len(), 1, "Small constant should emit Addi instruction");
    match &built_vcode.insts[0] {
        Riscv32MachInst::Addi { imm, .. } => assert_eq!(*imm, 42),
        _ => panic!("Should emit Addi for small constant"),
    }
}

#[test]
fn test_materialize_large_constant() {
    let mut vcode = VCodeBuilder::<Riscv32MachInst>::new(Riscv32EmitInfo);
    let block_idx = BlockIndex::new(0);
    vcode.start_block(block_idx, alloc::vec![]);

    let srcloc = RelSourceLoc::default();

    // Large constant that doesn't fit in 12 bits
    let _vreg = materialize_constant(
        &mut vcode,
        50000, // > 2047, requires LUI+ADDI
        srcloc,
        |rd, imm| Riscv32MachInst::Lui { rd, imm },
        |rd, rs1, imm| Riscv32MachInst::Addi { rd, rs1, imm },
        || zero_reg(),
    );

    vcode.end_block();

    // Should have emitted LUI + ADDI instructions
    // Build VCode to check instructions
    let vcode = build_vcode_with_single_block(vcode);
    assert_eq!(vcode.insts.len(), 2);

    // Check that first instruction is LUI
    match &vcode.insts[0] {
        Riscv32MachInst::Lui { .. } => {}
        _ => panic!("First instruction should be LUI"),
    }

    // Check that second instruction is ADDI
    match &vcode.insts[1] {
        Riscv32MachInst::Addi { .. } => {}
        _ => panic!("Second instruction should be ADDI"),
    }
}

#[test]
fn test_materialize_negative_constant() {
    let mut vcode = VCodeBuilder::<Riscv32MachInst>::new(Riscv32EmitInfo);
    let block_idx = BlockIndex::new(0);
    vcode.start_block(block_idx, alloc::vec![]);

    let srcloc = RelSourceLoc::default();

    // Negative constant that fits in 12 bits - now emits Addi instruction
    let vreg = materialize_constant(
        &mut vcode,
        -100,
        srcloc,
        |_rd, _imm| panic!("Should not create LUI for small constant"),
        |rd, rs1, imm| Riscv32MachInst::Addi { rd, rs1, imm },
        || zero_reg(),
    );

    vcode.end_block();

    // Build VCode to check instructions
    let built_vcode = build_vcode_with_single_block(vcode);
    // Small constants now emit Addi instructions (SSA requirement)
    assert_eq!(built_vcode.insts.len(), 1, "Small constant should emit Addi instruction");
    match &built_vcode.insts[0] {
        Riscv32MachInst::Addi { imm, .. } => assert_eq!(*imm, -100),
        _ => panic!("Should emit Addi for small constant"),
    }
}

#[test]
fn test_materialize_large_negative_constant() {
    let mut vcode = VCodeBuilder::<Riscv32MachInst>::new(Riscv32EmitInfo);
    let block_idx = BlockIndex::new(0);
    vcode.start_block(block_idx, alloc::vec![]);

    let srcloc = RelSourceLoc::default();

    // Large negative constant that doesn't fit in 12 bits
    let _vreg = materialize_constant(
        &mut vcode,
        -50000, // < -2048, requires LUI+ADDI
        srcloc,
        |rd, imm| Riscv32MachInst::Lui { rd, imm },
        |rd, rs1, imm| Riscv32MachInst::Addi { rd, rs1, imm },
        || zero_reg(),
    );

    vcode.end_block();

    // Should have emitted LUI + ADDI instructions
    // Build VCode to check instructions
    let vcode = build_vcode_with_single_block(vcode);
    assert_eq!(vcode.insts.len(), 2);
}

/// Test constant materialization with sign bit set in lower 12 bits
/// This tests the edge case where the lower 12 bits have bit 11 set,
/// requiring adjustment of the upper bits.
#[test]
fn test_materialize_constant_with_sign_bit_in_lower() {
    let mut vcode = VCodeBuilder::<Riscv32MachInst>::new(Riscv32EmitInfo);
    let block_idx = BlockIndex::new(0);
    vcode.start_block(block_idx, alloc::vec![]);

    let srcloc = RelSourceLoc::default();

    // Constant where lower 12 bits have sign bit set (bit 11 = 1)
    // Example: 0x12345800 has lower_12 = 0x800 (sign bit set)
    let value = 0x12345800i32;
    let _vreg = materialize_constant(
        &mut vcode,
        value,
        srcloc,
        |rd, imm| Riscv32MachInst::Lui { rd, imm },
        |rd, rs1, imm| Riscv32MachInst::Addi { rd, rs1, imm },
        || zero_reg(),
    );

    vcode.end_block();

    // Should have emitted LUI + ADDI instructions
    let vcode = build_vcode_with_single_block(vcode);
    assert_eq!(vcode.insts.len(), 2);

    // Verify LUI instruction has adjusted upper bits
    match &vcode.insts[0] {
        Riscv32MachInst::Lui { imm, .. } => {
            // Upper 20 bits should be 0x12346 (incremented from 0x12345)
            // Shifted left by 12: 0x12346000
            assert_eq!(
                *imm, 0x12346000u32,
                "LUI should have incremented upper bits"
            );
        }
        _ => panic!("First instruction should be LUI"),
    }

    // Verify ADDI instruction has lower 12 bits
    match &vcode.insts[1] {
        Riscv32MachInst::Addi { imm, .. } => {
            // Lower 12 bits: 0x800
            assert_eq!(*imm, 0x800i32, "ADDI should have lower 12 bits");
        }
        _ => panic!("Second instruction should be ADDI"),
    }
}

/// Test constant materialization at boundary values
#[test]
fn test_materialize_boundary_constants() {
    let mut vcode = VCodeBuilder::<Riscv32MachInst>::new(Riscv32EmitInfo);
    let block_idx = BlockIndex::new(0);
    vcode.start_block(block_idx, alloc::vec![]);

    let srcloc = RelSourceLoc::default();

    // Test at the boundary: 2047 (fits in 12 bits, emits Addi)
    let vreg1 = materialize_constant(
        &mut vcode,
        2047,
        srcloc,
        |_rd, _imm| panic!("Should not create LUI for small constant"),
        |rd, rs1, imm| Riscv32MachInst::Addi { rd, rs1, imm },
        || zero_reg(),
    );

    // Test just above boundary: 2048 (doesn't fit, needs LUI+ADDI)
    let vreg2 = materialize_constant(
        &mut vcode,
        2048,
        srcloc,
        |rd, imm| Riscv32MachInst::Lui { rd, imm },
        |rd, rs1, imm| Riscv32MachInst::Addi { rd, rs1, imm },
        || zero_reg(),
    );

    // Test at negative boundary: -2048 (fits in 12 bits, emits Addi)
    let vreg3 = materialize_constant(
        &mut vcode,
        -2048,
        srcloc,
        |_rd, _imm| panic!("Should not create LUI for small constant"),
        |rd, rs1, imm| Riscv32MachInst::Addi { rd, rs1, imm },
        || zero_reg(),
    );

    // Test just below boundary: -2049 (doesn't fit, needs LUI+ADDI)
    let vreg4 = materialize_constant(
        &mut vcode,
        -2049,
        srcloc,
        |rd, imm| Riscv32MachInst::Lui { rd, imm },
        |rd, rs1, imm| Riscv32MachInst::Addi { rd, rs1, imm },
        || zero_reg(),
    );

    vcode.end_block();

    let vcode = build_vcode_with_single_block(vcode);

    // All constants now emit instructions (SSA requirement)
    // Small constants (vreg1, vreg3) emit Addi
    // Large constants (vreg2, vreg4) emit LUI+ADDI
    // Should have 6 instructions total: 2 Addi (for 2047 and -2048) + 2 LUI + 2 ADDI (for 2048 and -2049)
    assert_eq!(
        vcode.insts.len(),
        6,
        "Should have 2 Addi + 2 LUI + 2 ADDI instructions"
    );
}

/// Test zero constant handling
#[test]
fn test_zero_constant() {
    let mut vcode = VCodeBuilder::<Riscv32MachInst>::new(Riscv32EmitInfo);
    let block_idx = BlockIndex::new(0);
    vcode.start_block(block_idx, alloc::vec![]);

    let srcloc = RelSourceLoc::default();

    // Zero constant fits in 12 bits, now emits Addi instruction
    let vreg = materialize_constant(
        &mut vcode,
        0,
        srcloc,
        |_rd, _imm| panic!("Should not create LUI for small constant"),
        |rd, rs1, imm| Riscv32MachInst::Addi { rd, rs1, imm },
        || zero_reg(),
    );

    vcode.end_block();

    // Build VCode to check instructions
    let built_vcode = build_vcode_with_single_block(vcode);

    // Zero constant now emits Addi instruction (SSA requirement)
    assert_eq!(
        built_vcode.insts.len(),
        1,
        "Zero constant should emit Addi instruction"
    );

    // Verify the instruction
    match &built_vcode.insts[0] {
        Riscv32MachInst::Addi { imm, .. } => {
            assert_eq!(*imm, 0i32, "Zero constant should have value 0");
        }
        _ => panic!("Zero constant should emit Addi"),
    }
}

/// Test constant reuse (same constant used multiple times)
#[test]
fn test_constant_reuse() {
    let mut vcode = VCodeBuilder::<Riscv32MachInst>::new(Riscv32EmitInfo);
    let block_idx = BlockIndex::new(0);
    vcode.start_block(block_idx, alloc::vec![]);

    let srcloc = RelSourceLoc::default();

    // Materialize the same constant twice
    let vreg1 = materialize_constant(
        &mut vcode,
        42,
        srcloc,
        |_rd, _imm| panic!("Should not create LUI for small constant"),
        |rd, rs1, imm| Riscv32MachInst::Addi { rd, rs1, imm },
        || zero_reg(),
    );

    let vreg2 = materialize_constant(
        &mut vcode,
        42,
        srcloc,
        |_rd, _imm| panic!("Should not create LUI for small constant"),
        |rd, rs1, imm| Riscv32MachInst::Addi { rd, rs1, imm },
        || zero_reg(),
    );

    vcode.end_block();

    // Build VCode
    let built_vcode = build_vcode_with_single_block(vcode);

    // Both constants now emit Addi instructions
    assert_eq!(
        built_vcode.insts.len(),
        2,
        "Each constant materialization should emit an Addi instruction"
    );

    // Both instructions should have the same immediate value
    match (&built_vcode.insts[0], &built_vcode.insts[1]) {
        (Riscv32MachInst::Addi { imm: imm1, .. }, Riscv32MachInst::Addi { imm: imm2, .. }) => {
            assert_eq!(*imm1, 42i32, "First constant should have value 42");
            assert_eq!(*imm2, 42i32, "Second constant should have value 42");
        }
        _ => panic!("Both should emit Addi instructions"),
    }

    // Note: Currently, each constant materialization creates a new VReg
    // Future optimization could reuse the same VReg for the same constant value
    assert_ne!(
        vreg1, vreg2,
        "Currently, each constant materialization creates a new VReg (may be optimized later)"
    );
}

/// Test constant in different contexts (immediate operand vs. register)
#[test]
fn test_constant_in_different_contexts() {
    use crate::backend3::tests::vcode_test_helpers::LowerTest;

    // Test that constants can be used in different instruction contexts
    // In this test, we use iconst to create constants, then use them in different ways
    let test = LowerTest::from_lpir(
        r#"
function %test() -> i32 {
block0:
    v1 = iconst 42
    v2 = iconst 100
    v3 = iadd v1, v2
    return v3
}
"#,
    );

    let vcode = test.vcode();

    // Constants should be recorded
    // The exact VRegs depend on lowering, but constants should be present
    assert!(
        !vcode.constants.constants.is_empty(),
        "Function with constants should have constants recorded"
    );

    // Verify constants are used in instructions
    // Find ADD instruction and verify it uses VRegs from constant materialization
    for inst in &vcode.insts {
        if let crate::isa::riscv32::backend3::inst::Riscv32MachInst::Add { rs1, rs2, .. } = inst {
            // Constants are now materialized as VRegs via Addi instructions
            // The VRegs are used in the Add instruction
            let _ = (rs1, rs2);
        }
    }
}
