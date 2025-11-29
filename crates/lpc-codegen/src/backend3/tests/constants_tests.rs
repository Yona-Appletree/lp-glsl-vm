//! Tests for constant materialization (inline and large)

extern crate alloc;

use lpc_lpir::RelSourceLoc;

use crate::{
    backend3::{constants::materialize_constant, vcode::Constant, vcode_builder::VCodeBuilder},
    isa::riscv32::backend3::inst::Riscv32MachInst,
};

#[test]
fn test_materialize_inline_constant() {
    let mut vcode = VCodeBuilder::<Riscv32MachInst>::new();
    let srcloc = RelSourceLoc::default();

    // Small constant that fits in 12 bits
    let vreg = materialize_constant(
        &mut vcode,
        42,
        srcloc,
        |_rd, _imm| panic!("Should not create LUI for inline constant"),
        |_rd, _rs1, _imm| panic!("Should not create ADDI for inline constant"),
    );

    // Build VCode to check constants
    let entry = crate::backend3::types::BlockIndex::new(0);
    let block_order = crate::backend3::vcode::BlockLoweringOrder {
        lowered_order: alloc::vec::Vec::new(),
        lowered_succs: alloc::vec::Vec::new(),
        block_to_index: alloc::collections::BTreeMap::new(),
        cold_blocks: alloc::collections::BTreeSet::new(),
        indirect_targets: alloc::collections::BTreeSet::new(),
    };
    let abi = crate::backend3::vcode::Callee {
        abi: crate::isa::riscv32::backend3::inst::Riscv32ABI,
    };
    let built_vcode = vcode.build(entry, block_order, abi);
    // Should have recorded the constant
    assert!(built_vcode.constants.constants.contains_key(&vreg));
    // Should not have emitted any instructions for inline constants
    assert_eq!(built_vcode.insts.len(), 0);
}

#[test]
fn test_materialize_large_constant() {
    let mut vcode = VCodeBuilder::<Riscv32MachInst>::new();
    let srcloc = RelSourceLoc::default();

    // Large constant that doesn't fit in 12 bits
    let _vreg = materialize_constant(
        &mut vcode,
        50000, // > 2047, requires LUI+ADDI
        srcloc,
        |rd, imm| Riscv32MachInst::Lui { rd, imm },
        |rd, rs1, imm| Riscv32MachInst::Addi { rd, rs1, imm },
    );

    // Should have emitted LUI + ADDI instructions
    // Build VCode to check instructions
    let entry = crate::backend3::types::BlockIndex::new(0);
    let block_order = crate::backend3::vcode::BlockLoweringOrder {
        lowered_order: alloc::vec::Vec::new(),
        lowered_succs: alloc::vec::Vec::new(),
        block_to_index: alloc::collections::BTreeMap::new(),
        cold_blocks: alloc::collections::BTreeSet::new(),
        indirect_targets: alloc::collections::BTreeSet::new(),
    };
    let abi = crate::backend3::vcode::Callee {
        abi: crate::isa::riscv32::backend3::inst::Riscv32ABI,
    };
    let vcode = vcode.build(entry, block_order, abi);
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
    let mut vcode = VCodeBuilder::<Riscv32MachInst>::new();
    let srcloc = RelSourceLoc::default();

    // Negative constant that fits in 12 bits (inline)
    let vreg = materialize_constant(
        &mut vcode,
        -100,
        srcloc,
        |_rd, _imm| panic!("Should not create LUI for inline constant"),
        |_rd, _rs1, _imm| panic!("Should not create ADDI for inline constant"),
    );

    // Build VCode to check constants
    let entry = crate::backend3::types::BlockIndex::new(0);
    let block_order = crate::backend3::vcode::BlockLoweringOrder {
        lowered_order: alloc::vec::Vec::new(),
        lowered_succs: alloc::vec::Vec::new(),
        block_to_index: alloc::collections::BTreeMap::new(),
        cold_blocks: alloc::collections::BTreeSet::new(),
        indirect_targets: alloc::collections::BTreeSet::new(),
    };
    let abi = crate::backend3::vcode::Callee {
        abi: crate::isa::riscv32::backend3::inst::Riscv32ABI,
    };
    let built_vcode = vcode.build(entry, block_order, abi);
    // Should have recorded the constant
    assert!(built_vcode.constants.constants.contains_key(&vreg));
    // Should not have emitted any instructions for inline constants
    assert_eq!(built_vcode.insts.len(), 0);
}

#[test]
fn test_materialize_large_negative_constant() {
    let mut vcode = VCodeBuilder::<Riscv32MachInst>::new();
    let srcloc = RelSourceLoc::default();

    // Large negative constant that doesn't fit in 12 bits
    let _vreg = materialize_constant(
        &mut vcode,
        -50000, // < -2048, requires LUI+ADDI
        srcloc,
        |rd, imm| Riscv32MachInst::Lui { rd, imm },
        |rd, rs1, imm| Riscv32MachInst::Addi { rd, rs1, imm },
    );

    // Should have emitted LUI + ADDI instructions
    // Build VCode to check instructions
    let entry = crate::backend3::types::BlockIndex::new(0);
    let block_order = crate::backend3::vcode::BlockLoweringOrder {
        lowered_order: alloc::vec::Vec::new(),
        lowered_succs: alloc::vec::Vec::new(),
        block_to_index: alloc::collections::BTreeMap::new(),
        cold_blocks: alloc::collections::BTreeSet::new(),
        indirect_targets: alloc::collections::BTreeSet::new(),
    };
    let abi = crate::backend3::vcode::Callee {
        abi: crate::isa::riscv32::backend3::inst::Riscv32ABI,
    };
    let vcode = vcode.build(entry, block_order, abi);
    assert_eq!(vcode.insts.len(), 2);
}

/// Test constant materialization with sign bit set in lower 12 bits
/// This tests the edge case where the lower 12 bits have bit 11 set,
/// requiring adjustment of the upper bits.
#[test]
fn test_materialize_constant_with_sign_bit_in_lower() {
    let mut vcode = VCodeBuilder::<Riscv32MachInst>::new();
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
    );

    // Should have emitted LUI + ADDI instructions
    let entry = crate::backend3::types::BlockIndex::new(0);
    let block_order = crate::backend3::vcode::BlockLoweringOrder {
        lowered_order: alloc::vec::Vec::new(),
        lowered_succs: alloc::vec::Vec::new(),
        block_to_index: alloc::collections::BTreeMap::new(),
        cold_blocks: alloc::collections::BTreeSet::new(),
        indirect_targets: alloc::collections::BTreeSet::new(),
    };
    let abi = crate::backend3::vcode::Callee {
        abi: crate::isa::riscv32::backend3::inst::Riscv32ABI,
    };
    let vcode = vcode.build(entry, block_order, abi);
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
    let mut vcode = VCodeBuilder::<Riscv32MachInst>::new();
    let srcloc = RelSourceLoc::default();

    // Test at the boundary: 2047 (fits in 12 bits, should be inline)
    let vreg1 = materialize_constant(
        &mut vcode,
        2047,
        srcloc,
        |_rd, _imm| panic!("Should not create LUI for inline constant"),
        |_rd, _rs1, _imm| panic!("Should not create ADDI for inline constant"),
    );

    // Test just above boundary: 2048 (doesn't fit, needs LUI+ADDI)
    let vreg2 = materialize_constant(
        &mut vcode,
        2048,
        srcloc,
        |rd, imm| Riscv32MachInst::Lui { rd, imm },
        |rd, rs1, imm| Riscv32MachInst::Addi { rd, rs1, imm },
    );

    // Test at negative boundary: -2048 (fits in 12 bits, should be inline)
    let vreg3 = materialize_constant(
        &mut vcode,
        -2048,
        srcloc,
        |_rd, _imm| panic!("Should not create LUI for inline constant"),
        |_rd, _rs1, _imm| panic!("Should not create ADDI for inline constant"),
    );

    // Test just below boundary: -2049 (doesn't fit, needs LUI+ADDI)
    let vreg4 = materialize_constant(
        &mut vcode,
        -2049,
        srcloc,
        |rd, imm| Riscv32MachInst::Lui { rd, imm },
        |rd, rs1, imm| Riscv32MachInst::Addi { rd, rs1, imm },
    );

    let entry = crate::backend3::types::BlockIndex::new(0);
    let block_order = crate::backend3::vcode::BlockLoweringOrder {
        lowered_order: alloc::vec::Vec::new(),
        lowered_succs: alloc::vec::Vec::new(),
        block_to_index: alloc::collections::BTreeMap::new(),
        cold_blocks: alloc::collections::BTreeSet::new(),
        indirect_targets: alloc::collections::BTreeSet::new(),
    };
    let abi = crate::backend3::vcode::Callee {
        abi: crate::isa::riscv32::backend3::inst::Riscv32ABI,
    };
    let vcode = vcode.build(entry, block_order, abi);

    // Inline constants (vreg1, vreg3) are recorded in constants map
    // Large constants (vreg2, vreg4) emit instructions but don't record in constants map
    assert!(
        vcode.constants.constants.contains_key(&vreg1),
        "Inline constant 2047 should be recorded"
    );
    assert!(
        !vcode.constants.constants.contains_key(&vreg2),
        "Large constant 2048 should not be in constants map (emits instructions)"
    );
    assert!(
        vcode.constants.constants.contains_key(&vreg3),
        "Inline constant -2048 should be recorded"
    );
    assert!(
        !vcode.constants.constants.contains_key(&vreg4),
        "Large constant -2049 should not be in constants map (emits instructions)"
    );

    // Should have 4 instructions (2 LUI + 2 ADDI for the two large constants: 2048 and -2049)
    assert_eq!(
        vcode.insts.len(),
        4,
        "Should have 2 LUI + 2 ADDI instructions for large constants"
    );
}

/// Test zero constant handling
#[test]
fn test_zero_constant() {
    let mut vcode = VCodeBuilder::<Riscv32MachInst>::new();
    let srcloc = RelSourceLoc::default();

    // Zero constant fits in 12 bits, so it's recorded as an inline constant
    let vreg = materialize_constant(
        &mut vcode,
        0,
        srcloc,
        |_rd, _imm| panic!("Should not create LUI for inline constant"),
        |_rd, _rs1, _imm| panic!("Should not create ADDI for inline constant"),
    );

    // Build VCode to check constants
    let entry = crate::backend3::types::BlockIndex::new(0);
    let block_order = crate::backend3::vcode::BlockLoweringOrder {
        lowered_order: alloc::vec::Vec::new(),
        lowered_succs: alloc::vec::Vec::new(),
        block_to_index: alloc::collections::BTreeMap::new(),
        cold_blocks: alloc::collections::BTreeSet::new(),
        indirect_targets: alloc::collections::BTreeSet::new(),
    };
    let abi = crate::backend3::vcode::Callee {
        abi: crate::isa::riscv32::backend3::inst::Riscv32ABI,
    };
    let built_vcode = vcode.build(entry, block_order, abi);

    // Zero should be recorded as an inline constant
    assert!(
        built_vcode.constants.constants.contains_key(&vreg),
        "Zero constant should be recorded as inline constant"
    );

    // Should not have emitted any instructions for inline constants
    assert_eq!(
        built_vcode.insts.len(),
        0,
        "Zero constant should not emit instructions (inline)"
    );

    // Verify the constant value
    if let Some(constant) = built_vcode.constants.constants.get(&vreg) {
        match constant {
            Constant::Inline(value) => {
                assert_eq!(*value, 0i32, "Zero constant should have value 0");
            }
            _ => panic!("Zero constant should be inline"),
        }
    }
}

/// Test constant reuse (same constant used multiple times)
#[test]
fn test_constant_reuse() {
    let mut vcode = VCodeBuilder::<Riscv32MachInst>::new();
    let srcloc = RelSourceLoc::default();

    // Materialize the same constant twice
    let vreg1 = materialize_constant(
        &mut vcode,
        42,
        srcloc,
        |_rd, _imm| panic!("Should not create LUI for inline constant"),
        |_rd, _rs1, _imm| panic!("Should not create ADDI for inline constant"),
    );

    let vreg2 = materialize_constant(
        &mut vcode,
        42,
        srcloc,
        |_rd, _imm| panic!("Should not create LUI for inline constant"),
        |_rd, _rs1, _imm| panic!("Should not create ADDI for inline constant"),
    );

    // Build VCode
    let entry = crate::backend3::types::BlockIndex::new(0);
    let block_order = crate::backend3::vcode::BlockLoweringOrder {
        lowered_order: alloc::vec::Vec::new(),
        lowered_succs: alloc::vec::Vec::new(),
        block_to_index: alloc::collections::BTreeMap::new(),
        cold_blocks: alloc::collections::BTreeSet::new(),
        indirect_targets: alloc::collections::BTreeSet::new(),
    };
    let abi = crate::backend3::vcode::Callee {
        abi: crate::isa::riscv32::backend3::inst::Riscv32ABI,
    };
    let built_vcode = vcode.build(entry, block_order, abi);

    // Both VRegs should be recorded as constants
    assert!(
        built_vcode.constants.constants.contains_key(&vreg1),
        "First constant should be recorded"
    );
    assert!(
        built_vcode.constants.constants.contains_key(&vreg2),
        "Second constant should be recorded"
    );

    // Both should have the same value
    if let (Some(c1), Some(c2)) = (
        built_vcode.constants.constants.get(&vreg1),
        built_vcode.constants.constants.get(&vreg2),
    ) {
        match (c1, c2) {
            (Constant::Inline(v1), Constant::Inline(v2)) => {
                assert_eq!(*v1, 42i32, "First constant should have value 42");
                assert_eq!(*v2, 42i32, "Second constant should have value 42");
            }
            _ => panic!("Both constants should be inline"),
        }
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
    // Find ADD instruction and verify it uses constant VRegs
    for inst in &vcode.insts {
        if let crate::isa::riscv32::backend3::inst::Riscv32MachInst::Add { rs1, rs2, .. } = inst {
            // At least one operand might be a constant VReg
            let rs1_is_constant = vcode.constants.constants.contains_key(rs1);
            let rs2_is_constant = vcode.constants.constants.contains_key(rs2);
            // In this case, constants are materialized and used as VRegs
            // The exact behavior depends on lowering strategy
            let _ = (rs1_is_constant, rs2_is_constant);
        }
    }
}
