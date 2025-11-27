//! Liveness analysis for register allocation.

extern crate alloc;

use alloc::{
    collections::{BTreeMap, BTreeSet},
    vec::Vec,
};

use lpc_lpir::{Function, Inst, Value};

/// Instruction point (block index, instruction index)
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InstPoint {
    pub block: usize,
    pub inst: usize,
}

/// Live range for a value (from definition to last use)
#[derive(Clone, Debug)]
pub struct LiveRange {
    /// Point where value is defined
    pub def: InstPoint,
    /// Point where value is last used
    pub last_use: InstPoint,
    /// All points where value is used
    pub uses: Vec<InstPoint>,
}

/// Liveness information for a function
pub struct LivenessInfo {
    /// Live range for each value
    pub live_ranges: BTreeMap<Value, LiveRange>,
    /// Set of live values at each instruction point
    pub live_sets: BTreeMap<InstPoint, BTreeSet<Value>>,
    /// Values defined at each instruction point
    pub defs: BTreeMap<InstPoint, Value>,
    /// Values used at each instruction point
    pub uses: BTreeMap<InstPoint, Vec<Value>>,
    /// Block parameters (phi-like values) - map from (block_idx, param_idx) to Value
    pub block_params: BTreeMap<(usize, usize), Value>,
}

/// Compute liveness for all values in a function
pub fn compute_liveness(func: &Function) -> LivenessInfo {
    // Step 1: Forward pass - collect all definitions
    let mut defs = BTreeMap::new();
    let mut uses = BTreeMap::new();
    let mut block_params = BTreeMap::new();

    // Handle function parameters (defined at block 0 entry)
    for (param_idx, param_value) in func.blocks[0].params.iter().enumerate() {
        block_params.insert((0, param_idx), *param_value);
        defs.insert(InstPoint { block: 0, inst: 0 }, *param_value);
    }

    // Iterate through all blocks and instructions
    for (block_idx, block) in func.blocks.iter().enumerate() {
        // Handle block parameters (defined at block entry)
        for (param_idx, param_value) in block.params.iter().enumerate() {
            block_params.insert((block_idx, param_idx), *param_value);
            // Block parameters are "defined" at block entry (before first instruction)
            defs.insert(
                InstPoint {
                    block: block_idx,
                    inst: 0,
                },
                *param_value,
            );
        }

        // Process instructions in the block
        for (inst_idx, inst) in block.insts.iter().enumerate() {
            let point = InstPoint {
                block: block_idx,
                inst: inst_idx + 1, // +1 because 0 is reserved for block entry
            };

            // Record definitions (skip Return instructions - they don't produce results)
            match inst {
                Inst::Return { .. } => {
                    // Return instructions don't define values, they only use them
                }
                _ => {
                    for result in inst.results() {
                        defs.insert(point, result);
                    }
                }
            }

            // Record uses
            let inst_uses = inst.args();
            if !inst_uses.is_empty() {
                uses.insert(point, inst_uses);
            }
        }
    }

    // Step 2: Backward pass - compute last uses and live ranges
    let mut last_uses: BTreeMap<Value, InstPoint> = BTreeMap::new();
    let mut all_uses: BTreeMap<Value, Vec<InstPoint>> = BTreeMap::new();

    // Find last use of each value by iterating backwards
    for (point, values) in uses.iter().rev() {
        for value in values {
            if !last_uses.contains_key(value) {
                last_uses.insert(*value, *point);
            }
            all_uses.entry(*value).or_insert_with(Vec::new).push(*point);
        }
    }

    // Also consider return statements and block exits as uses
    for (block_idx, block) in func.blocks.iter().enumerate() {
        // Check if block ends with return or branches
        if let Some(last_inst) = block.insts.last() {
            let point = InstPoint {
                block: block_idx,
                inst: block.insts.len(), // After last instruction
            };
            match last_inst {
                Inst::Return { values } => {
                    for value in values {
                        if !last_uses.contains_key(value) {
                            last_uses.insert(*value, point);
                        }
                        all_uses.entry(*value).or_insert_with(Vec::new).push(point);
                    }
                }
                _ => {}
            }
        }
    }

    // Step 3: Build live ranges
    let mut live_ranges = BTreeMap::new();
    for (def_point, value) in &defs {
        let last_use = last_uses.get(value).copied().unwrap_or(*def_point);
        let uses_list = all_uses.get(value).cloned().unwrap_or_default();

        live_ranges.insert(
            *value,
            LiveRange {
                def: *def_point,
                last_use,
                uses: uses_list,
            },
        );
    }

    // Step 4: Build live sets for each instruction point
    let mut live_sets = BTreeMap::new();
    for (value, live_range) in &live_ranges {
        // Value is live at all points from def to last_use (inclusive)
        let start_block = live_range.def.block;
        let end_block = live_range.last_use.block;

        // For simplicity, mark value as live at all instruction points in its live range
        // This is a conservative approximation
        for block_idx in start_block..=end_block {
            if block_idx < func.blocks.len() {
                let block = &func.blocks[block_idx];
                let start_inst = if block_idx == start_block {
                    live_range.def.inst
                } else {
                    0
                };
                let end_inst = if block_idx == end_block {
                    live_range.last_use.inst
                } else {
                    block.insts.len()
                };

                for inst_idx in start_inst..=end_inst {
                    let point = InstPoint {
                        block: block_idx,
                        inst: inst_idx,
                    };
                    live_sets
                        .entry(point)
                        .or_insert_with(BTreeSet::new)
                        .insert(*value);
                }
            }
        }
    }

    LivenessInfo {
        live_ranges,
        live_sets,
        defs,
        uses,
        block_params,
    }
}

#[cfg(test)]
mod tests {
    use lpc_lpir::parse_function;

    use super::*;

    #[test]
    fn test_liveness_simple_sequential() {
        let ir = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 42
    return v1
}"#;

        let func = parse_function(ir).expect("Failed to parse IR");
        let liveness = compute_liveness(&func);

        // v0 is a parameter, defined at block 0 entry
        let v0 = Value::new(0);
        assert!(liveness.live_ranges.contains_key(&v0));

        // v1 is defined and used in return
        let v1 = Value::new(1);
        assert!(liveness.live_ranges.contains_key(&v1));
    }

    #[test]
    fn test_liveness_multiple_uses() {
        // Test a value used multiple times: v0 + v0
        let ir = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iadd v0, v0
    return v1
}"#;

        let func = parse_function(ir).expect("Failed to parse IR");
        let liveness = compute_liveness(&func);
        let v0 = Value::new(0);
        let v0_range = liveness.live_ranges.get(&v0).unwrap();

        // v0 should be used twice (once for each arg in iadd)
        assert!(v0_range.uses.len() >= 2);
    }

    #[test]
    fn test_liveness_unused_value() {
        // Test a value that is defined but never used
        let ir = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 42
    return v0
}"#;

        let func = parse_function(ir).expect("Failed to parse IR");
        let liveness = compute_liveness(&func);
        let v1 = Value::new(1);

        // v1 should have a live range even if unused
        let v1_range = liveness.live_ranges.get(&v1);
        assert!(v1_range.is_some());

        // Its last use should be the same as its def (no uses)
        let range = v1_range.unwrap();
        assert_eq!(range.def, range.last_use);
    }

    #[test]
    fn test_liveness_block_parameters() {
        // Test block parameters (phi-like values)
        let ir = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 1
    brif v1, block1, block1

block1(v2: i32):
    return v2
}"#;

        let func = parse_function(ir).expect("Failed to parse IR");
        let liveness = compute_liveness(&func);

        // Block 0 parameter
        let v0 = Value::new(0);
        assert!(liveness.block_params.contains_key(&(0, 0)));
        assert_eq!(liveness.block_params.get(&(0, 0)), Some(&v0));

        // Block 1 parameter
        let v2 = Value::new(2);
        assert!(liveness.block_params.contains_key(&(1, 0)));
        assert_eq!(liveness.block_params.get(&(1, 0)), Some(&v2));
    }

    #[test]
    fn test_liveness_across_blocks() {
        // Test a value that is live across multiple blocks
        let ir = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 42
    v2 = iconst 1
    brif v2, block1, block2

block1:
    return v1

block2:
    return v1
}"#;

        let func = parse_function(ir).expect("Failed to parse IR");
        let liveness = compute_liveness(&func);
        let v1 = Value::new(1);

        // v1 should have a live range
        assert!(
            liveness.live_ranges.contains_key(&v1),
            "v1 should have a live range"
        );

        let v1_range = liveness.live_ranges.get(&v1).unwrap();
        // v1 should be defined in block 0 at instruction 1 (where iconst 42 is)
        assert_eq!(
            v1_range.def.block, 0,
            "v1 should be defined in block 0, got block {}",
            v1_range.def.block
        );
        // Last use should be in block 1 or 2 (both blocks use v1 in return statements)
        assert!(
            v1_range.last_use.block >= 1,
            "last use should be in block 1 or 2, got block {}",
            v1_range.last_use.block
        );
        // Should have uses in both blocks 1 and 2
        assert!(!v1_range.uses.is_empty(), "v1 should have uses");
        // v1 should span multiple blocks (defined in block 0, used in blocks 1 and 2)
        assert_ne!(
            v1_range.def.block, v1_range.last_use.block,
            "v1 should span multiple blocks (def in {}, last use in {})",
            v1_range.def.block, v1_range.last_use.block
        );
    }

    #[test]
    fn test_liveness_loop() {
        // Test a value live across blocks (simplified loop pattern)
        let ir = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 0
    jump block1

block1:
    v2 = iadd v1, v0
    return v2
}"#;

        let func = parse_function(ir).expect("Failed to parse IR");
        let liveness = compute_liveness(&func);
        let v1 = Value::new(1);
        let v1_range = liveness.live_ranges.get(&v1).unwrap();

        // v1 should be live from block 0 into block 1
        assert_eq!(v1_range.def.block, 0);
        assert!(v1_range.last_use.block >= 1);
    }

    #[test]
    fn test_liveness_sequential_chain() {
        // Test a chain: v2 = v0 + 1, v4 = v2 + 1
        let ir = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 1
    v2 = iadd v0, v1
    v3 = iconst 1
    v4 = iadd v2, v3
    return v4
}"#;

        let func = parse_function(ir).expect("Failed to parse IR");
        let liveness = compute_liveness(&func);

        // Check that all values have live ranges
        for i in 0..=4 {
            let val = Value::new(i);
            assert!(
                liveness.live_ranges.contains_key(&val),
                "Value {} should have a live range",
                i
            );
        }

        // v0 should be live until v2 is computed
        let v0_range = liveness.live_ranges.get(&Value::new(0)).unwrap();
        assert_eq!(v0_range.def.block, 0);
        assert_eq!(v0_range.def.inst, 0); // Parameter defined at block entry

        // v2 should be live until v4 is computed
        let v2_range = liveness.live_ranges.get(&Value::new(2)).unwrap();
        assert_eq!(v2_range.def.block, 0);
    }

    #[test]
    fn test_liveness_branch_condition() {
        // Test value used in branch condition
        let ir = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 1
    brif v0, block1, block2

block1:
    return v1

block2:
    return v1
}"#;

        let func = parse_function(ir).expect("Failed to parse IR");
        let liveness = compute_liveness(&func);
        let v0 = Value::new(0);

        // v0 should be used in the branch condition
        let v0_range = liveness.live_ranges.get(&v0).unwrap();
        assert_eq!(v0_range.def.block, 0);
        assert_eq!(v0_range.def.inst, 0); // Parameter
                                          // v0 should be used at the brif instruction
        assert!(!v0_range.uses.is_empty());
    }

    #[test]
    fn test_liveness_load_store() {
        // Test load and store instructions
        let ir = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = load.i32 v0
    store.i32 v0, v1
    return v1
}"#;

        let func = parse_function(ir).expect("Failed to parse IR");
        let liveness = compute_liveness(&func);

        // v0 should be used in load and store
        let v0_range = liveness.live_ranges.get(&Value::new(0)).unwrap();
        assert!(!v0_range.uses.is_empty());

        // v1 should be defined by load and used in store and return
        let v1_range = liveness.live_ranges.get(&Value::new(1)).unwrap();
        assert_eq!(v1_range.def.block, 0);
        assert!(!v1_range.uses.is_empty());
    }

    #[test]
    fn test_liveness_comparison() {
        // Test comparison instructions
        let ir = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 42
    v2 = icmp_eq v0, v1
    return v2
}"#;

        let func = parse_function(ir).expect("Failed to parse IR");
        let liveness = compute_liveness(&func);

        // v0 should be used in comparison
        let v0_range = liveness.live_ranges.get(&Value::new(0)).unwrap();
        assert!(!v0_range.uses.is_empty());

        // v1 should be used in comparison
        let v1_range = liveness.live_ranges.get(&Value::new(1)).unwrap();
        assert!(!v1_range.uses.is_empty());

        // v2 should be defined by comparison
        let v2_range = liveness.live_ranges.get(&Value::new(2)).unwrap();
        assert_eq!(v2_range.def.block, 0);
    }

    #[test]
    fn test_liveness_multiple_uses_same_block() {
        // Test value used multiple times in different instructions within same block
        let ir = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iadd v0, v0
    v2 = iadd v1, v0
    v3 = iadd v1, v1
    return v3
}"#;

        let func = parse_function(ir).expect("Failed to parse IR");
        let liveness = compute_liveness(&func);

        // v0 should be used multiple times
        let v0_range = liveness.live_ranges.get(&Value::new(0)).unwrap();
        assert!(v0_range.uses.len() >= 3);

        // v1 should be used multiple times (in v2 and v3)
        let v1_range = liveness.live_ranges.get(&Value::new(1)).unwrap();
        assert!(v1_range.uses.len() >= 2);
    }

    #[test]
    fn test_liveness_empty_block() {
        // Test block with only return (no other instructions)
        let ir = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    return v0
}"#;

        let func = parse_function(ir).expect("Failed to parse IR");
        let liveness = compute_liveness(&func);

        // v0 should be used in return
        let v0_range = liveness.live_ranges.get(&Value::new(0)).unwrap();
        assert_eq!(v0_range.def.block, 0);
        assert_eq!(v0_range.def.inst, 0); // Parameter
        assert!(!v0_range.uses.is_empty());
    }

    #[test]
    fn test_liveness_multiple_block_params() {
        // Test block with multiple parameters
        let ir = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 1
    brif v1, block1, block1

block1(v2: i32, v3: i32):
    v4 = iadd v2, v3
    return v4
}"#;

        let func = parse_function(ir).expect("Failed to parse IR");
        let liveness = compute_liveness(&func);

        // Block 1 should have two parameters
        assert!(liveness.block_params.contains_key(&(1, 0)));
        assert!(liveness.block_params.contains_key(&(1, 1)));

        let v2 = Value::new(2);
        let v3 = Value::new(3);
        assert_eq!(liveness.block_params.get(&(1, 0)), Some(&v2));
        assert_eq!(liveness.block_params.get(&(1, 1)), Some(&v3));
    }

    #[test]
    fn test_liveness_call_instruction() {
        // Test function call instruction
        let ir = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    call %other(v0) -> v2
    return v2
}"#;

        let func = parse_function(ir).expect("Failed to parse IR");
        let liveness = compute_liveness(&func);

        // v0 should be used as argument to call
        let v0_range = liveness.live_ranges.get(&Value::new(0)).unwrap();
        assert!(!v0_range.uses.is_empty());

        // v2 should be defined by call result
        let v2_range = liveness.live_ranges.get(&Value::new(2)).unwrap();
        assert_eq!(v2_range.def.block, 0);
    }

    #[test]
    fn test_liveness_syscall() {
        // Test syscall instruction
        let ir = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    syscall 1(v0)
    return v0
}"#;

        let func = parse_function(ir).expect("Failed to parse IR");
        let liveness = compute_liveness(&func);

        // v0 should be used in syscall and return
        let v0_range = liveness.live_ranges.get(&Value::new(0)).unwrap();
        assert!(v0_range.uses.len() >= 2);
    }

    #[test]
    fn test_liveness_complex_control_flow() {
        // Test complex nested control flow
        let ir = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 1
    v2 = iconst 2
    brif v1, block1, block2

block1:
    brif v2, block3, block4

block2:
    return v0

block3:
    return v0

block4:
    return v0
}"#;

        let func = parse_function(ir).expect("Failed to parse IR");
        let liveness = compute_liveness(&func);

        // v0 should be live across multiple blocks
        let v0_range = liveness.live_ranges.get(&Value::new(0)).unwrap();
        assert_eq!(v0_range.def.block, 0);
        // v0 is used in blocks 2, 3, and 4
        assert!(v0_range.last_use.block >= 2);
        assert!(!v0_range.uses.is_empty());

        // v1 should be used in first branch
        let v1_range = liveness.live_ranges.get(&Value::new(1)).unwrap();
        assert!(!v1_range.uses.is_empty());

        // v2 should be used in second branch
        let v2_range = liveness.live_ranges.get(&Value::new(2)).unwrap();
        assert!(!v2_range.uses.is_empty());
    }

    #[test]
    fn test_liveness_live_sets() {
        // Test that live_sets are correctly populated
        let ir = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 42
    v2 = iadd v0, v1
    return v2
}"#;

        let func = parse_function(ir).expect("Failed to parse IR");
        let liveness = compute_liveness(&func);

        // Check that live_sets contains entries
        assert!(!liveness.live_sets.is_empty());

        // v0 should be live at block 0, inst 0 (parameter)
        let entry_point = InstPoint { block: 0, inst: 0 };
        assert!(liveness.live_sets.contains_key(&entry_point));

        // v0 should be live at the iadd instruction
        let add_point = InstPoint { block: 0, inst: 2 };
        let live_at_add = liveness.live_sets.get(&add_point);
        assert!(live_at_add.is_some());
        assert!(live_at_add.unwrap().contains(&Value::new(0)));

        // v1 should be live at the iadd instruction
        assert!(live_at_add.unwrap().contains(&Value::new(1)));

        // v2 should be live at the return
        let return_point = InstPoint { block: 0, inst: 3 };
        let live_at_return = liveness.live_sets.get(&return_point);
        assert!(live_at_return.is_some());
        assert!(live_at_return.unwrap().contains(&Value::new(2)));
    }

    #[test]
    fn test_liveness_defs_and_uses_maps() {
        // Test that defs and uses maps are correctly populated
        let ir = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 42
    v2 = iadd v0, v1
    return v2
}"#;

        let func = parse_function(ir).expect("Failed to parse IR");
        let liveness = compute_liveness(&func);

        // v0 should be in defs at block entry
        let entry_point = InstPoint { block: 0, inst: 0 };
        assert_eq!(liveness.defs.get(&entry_point), Some(&Value::new(0)));

        // v1 should be in defs at instruction 1
        let v1_def_point = InstPoint { block: 0, inst: 1 };
        assert_eq!(liveness.defs.get(&v1_def_point), Some(&Value::new(1)));

        // v2 should be in defs at instruction 2
        let v2_def_point = InstPoint { block: 0, inst: 2 };
        assert_eq!(liveness.defs.get(&v2_def_point), Some(&Value::new(2)));

        // v0 and v1 should be in uses at instruction 2 (iadd)
        let add_uses = liveness.uses.get(&v2_def_point);
        assert!(add_uses.is_some());
        let uses = add_uses.unwrap();
        assert!(uses.contains(&Value::new(0)));
        assert!(uses.contains(&Value::new(1)));

        // v2 should be in uses at return
        let return_point = InstPoint { block: 0, inst: 3 };
        let return_uses = liveness.uses.get(&return_point);
        assert!(return_uses.is_some());
        assert!(return_uses.unwrap().contains(&Value::new(2)));
    }

    #[test]
    fn test_liveness_value_used_before_defined_in_block() {
        // Test that values used before being defined in same block are handled
        // (This shouldn't happen in SSA, but test the behavior)
        let ir = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iadd v0, v0
    v2 = iadd v1, v0
    return v2
}"#;

        let func = parse_function(ir).expect("Failed to parse IR");
        let liveness = compute_liveness(&func);

        // v0 should be defined before v1 uses it
        let v0_range = liveness.live_ranges.get(&Value::new(0)).unwrap();
        let v1_range = liveness.live_ranges.get(&Value::new(1)).unwrap();

        // v0's definition should come before v1's first use
        assert!(v0_range.def.block <= v1_range.def.block);
        if v0_range.def.block == v1_range.def.block {
            assert!(v0_range.def.inst <= v1_range.def.inst);
        }
    }

    #[test]
    fn test_liveness_jump_instruction() {
        // Test jump instruction (no values used)
        let ir = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 42
    jump block1

block1:
    return v1
}"#;

        let func = parse_function(ir).expect("Failed to parse IR");
        let liveness = compute_liveness(&func);

        // v1 should be live across blocks
        let v1_range = liveness.live_ranges.get(&Value::new(1)).unwrap();
        assert_eq!(v1_range.def.block, 0);
        assert!(v1_range.last_use.block >= 1);
    }
}
