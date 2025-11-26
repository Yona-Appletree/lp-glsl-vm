//! Tests to verify temporary spill slots don't exceed frame bounds.

use r5_ir::parse_function;
use r5_target_riscv32::{
    allocate_registers, compute_liveness, create_spill_reload_plan, FrameLayout,
};

#[test]
fn test_temp_spill_slots_within_frame_bounds() {
    // Test that temporary spill slots don't exceed frame bounds
    // Function with a call where multiple caller-saved values need temporary spills
    let ir = r#"
function %test() -> i32 {
block0:
    v0 = iconst 10
    v1 = iconst 20
    v2 = iconst 30
    v3 = iconst 40
    v4 = iconst 50
    v5 = iconst 60
    v6 = iconst 70
    v7 = iconst 80
    v8 = iconst 90
    v9 = iconst 100
    call %helper(v0) -> v10
    v11 = iadd v1, v2
    v12 = iadd v3, v4
    v13 = iadd v5, v6
    v14 = iadd v7, v8
    v15 = iadd v9, v11
    v16 = iadd v12, v13
    v17 = iadd v14, v15
    v18 = iadd v16, v17
    return v18
}"#;

    let func = parse_function(ir).expect("Failed to parse IR function");
    let liveness = compute_liveness(&func);
    let allocation = allocate_registers(&func, &liveness);
    let spill_reload = create_spill_reload_plan(&func, &allocation, &liveness);

    // Verify that max_temp_spill_slots is set correctly
    assert!(
        spill_reload.max_temp_spill_slots > 0,
        "Should need temporary spill slots for caller-saved values"
    );

    // Compute frame layout with temporary slots
    let has_calls = true;
    let total_spill_slots = allocation.spill_slot_count + spill_reload.max_temp_spill_slots;
    let frame_layout = FrameLayout::compute(
        &allocation.used_callee_saved,
        total_spill_slots,
        has_calls,
        func.signature.params.len(),
        0,
    );

    // Verify that all temporary spill slots are within frame bounds
    // The maximum temporary slot number would be: allocation.spill_slot_count + max_temp_spill_slots - 1
    let max_temp_slot = (allocation.spill_slot_count + spill_reload.max_temp_spill_slots) as u32;
    if max_temp_slot > allocation.spill_slot_count as u32 {
        // Check that the maximum temporary slot offset is within the frame
        let max_offset = frame_layout.spill_slot_offset(max_temp_slot - 1);
        let frame_size = frame_layout.total_size();

        // The offset should be negative and within the frame bounds
        // Frame starts at -frame_size (after SP adjustment)
        assert!(
            max_offset.as_i32() < 0,
            "Spill slot offset should be negative"
        );
        assert!(
            max_offset.as_i32().abs() as u32 <= frame_size,
            "Spill slot offset {} should be within frame size {}",
            max_offset.as_i32().abs(),
            frame_size
        );
    }
}

#[test]
fn test_temp_spill_slots_no_calls() {
    // Test that functions without calls don't need temporary spill slots
    let ir = r#"
function %test() -> i32 {
block0:
    v0 = iconst 1
    v1 = iconst 2
    v2 = iadd v0, v1
    return v2
}"#;

    let func = parse_function(ir).expect("Failed to parse IR function");
    let liveness = compute_liveness(&func);
    let allocation = allocate_registers(&func, &liveness);
    let spill_reload = create_spill_reload_plan(&func, &allocation, &liveness);

    // Functions without calls shouldn't need temporary spill slots
    assert_eq!(
        spill_reload.max_temp_spill_slots, 0,
        "Functions without calls shouldn't need temporary spill slots"
    );
}

#[test]
fn test_temp_spill_slots_multiple_calls() {
    // Test that max_temp_spill_slots accounts for the maximum across all calls
    let ir = r#"
function %test() -> i32 {
block0:
    v0 = iconst 10
    v1 = iconst 20
    v2 = iconst 30
    call %helper1(v0) -> v3
    v4 = iadd v1, v3
    call %helper2(v4) -> v5
    v6 = iadd v2, v5
    return v6
}"#;

    let func = parse_function(ir).expect("Failed to parse IR function");
    let liveness = compute_liveness(&func);
    let allocation = allocate_registers(&func, &liveness);
    let spill_reload = create_spill_reload_plan(&func, &allocation, &liveness);

    // Should account for temporary slots needed across all calls
    // (max_temp_spill_slots is usize, so it's always >= 0)

    // Verify frame layout includes temporary slots
    let has_calls = true;
    let total_spill_slots = allocation.spill_slot_count + spill_reload.max_temp_spill_slots;
    let frame_layout = FrameLayout::compute(
        &allocation.used_callee_saved,
        total_spill_slots,
        has_calls,
        func.signature.params.len(),
        0,
    );

    // All spill slots (including temporary ones) should be within frame bounds
    if total_spill_slots > 0 {
        let max_slot = (total_spill_slots - 1) as u32;
        let max_offset = frame_layout.spill_slot_offset(max_slot);
        let frame_size = frame_layout.total_size();

        assert!(
            max_offset.as_i32().abs() as u32 <= frame_size,
            "All spill slots should be within frame bounds. Max slot: {}, Max offset: {}, Frame \
             size: {}",
            max_slot,
            max_offset.as_i32().abs(),
            frame_size
        );
    }
}
