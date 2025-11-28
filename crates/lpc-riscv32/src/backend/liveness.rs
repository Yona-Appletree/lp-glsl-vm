//! Liveness analysis for register allocation.

extern crate alloc;

use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::String,
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
    pub defs: BTreeMap<InstPoint, BTreeSet<Value>>,
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
        defs.entry(InstPoint { block: 0, inst: 0 })
            .or_insert_with(BTreeSet::new)
            .insert(*param_value);
    }

    // Iterate through all blocks and instructions
    for (block_idx, block) in func.blocks.iter().enumerate() {
        // Handle block parameters (defined at block entry)
        for (param_idx, param_value) in block.params.iter().enumerate() {
            block_params.insert((block_idx, param_idx), *param_value);
            // Block parameters are "defined" at block entry (before first instruction)
            defs.entry(InstPoint {
                block: block_idx,
                inst: 0,
            })
            .or_insert_with(BTreeSet::new)
            .insert(*param_value);
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
                        defs.entry(point)
                            .or_insert_with(BTreeSet::new)
                            .insert(result);
                    }
                }
            }

            // Record uses
            // Note: inst.args() already includes jump/branch args, so values passed
            // between blocks are correctly tracked as uses at the jump/branch point
            let inst_uses = inst.args();
            if !inst_uses.is_empty() {
                uses.insert(point, inst_uses);
            }
        }
    }

    // Step 2: Backward pass - compute last uses
    let mut last_uses: BTreeMap<Value, InstPoint> = BTreeMap::new();
    let mut all_uses: BTreeMap<Value, Vec<InstPoint>> = BTreeMap::new();

    // Find last use of each value by iterating backwards through uses
    // This gives us the last point where each value is used
    for (point, values) in uses.iter().rev() {
        for value in values {
            if !last_uses.contains_key(value) {
                last_uses.insert(*value, *point);
            }
            all_uses.entry(*value).or_insert_with(Vec::new).push(*point);
        }
    }

    // Step 3: Build live ranges
    let mut live_ranges = BTreeMap::new();
    for (def_point, values) in &defs {
        for value in values {
            let last_use = last_uses.get(value).copied().unwrap_or(*def_point);
            let mut uses_list = all_uses.get(value).cloned().unwrap_or_default();
            // Sort uses for consistency
            uses_list.sort();

            live_ranges.insert(
                *value,
                LiveRange {
                    def: *def_point,
                    last_use,
                    uses: uses_list,
                },
            );
        }
    }

    // Step 4: Build live sets - precise tracking instead of conservative approximation
    // A value is live at a point if:
    // 1. The point is between def and last_use (inclusive)
    // 2. The value is actually used at or after that point
    let mut live_sets = BTreeMap::new();

    for (value, live_range) in &live_ranges {
        // Mark value as live at its definition point
        live_sets
            .entry(live_range.def)
            .or_insert_with(BTreeSet::new)
            .insert(*value);

        // Mark value as live at all use points
        for &use_point in &live_range.uses {
            live_sets
                .entry(use_point)
                .or_insert_with(BTreeSet::new)
                .insert(*value);
        }

        // Mark value as live at all points between def and last_use within the same block
        // For cross-block values, only mark live in blocks where it's actually used
        let def_block = live_range.def.block;
        let last_use_block = live_range.last_use.block;

        if def_block == last_use_block {
            // Value is used within the same block - mark live at all points from def to last_use
            let start_inst = live_range.def.inst;
            let end_inst = live_range.last_use.inst;

            for inst_idx in start_inst..=end_inst {
                let point = InstPoint {
                    block: def_block,
                    inst: inst_idx,
                };
                live_sets
                    .entry(point)
                    .or_insert_with(BTreeSet::new)
                    .insert(*value);
            }
        } else {
            // Value spans multiple blocks - mark live only in blocks where it's used
            // In the definition block: live from def to end of block
            let start_inst = live_range.def.inst;
            let end_inst = func.blocks[def_block].insts.len();

            for inst_idx in start_inst..=end_inst {
                let point = InstPoint {
                    block: live_range.def.block,
                    inst: inst_idx,
                };
                live_sets
                    .entry(point)
                    .or_insert_with(BTreeSet::new)
                    .insert(*value);
            }

            // In blocks where value is used (via jump/branch args or within the block)
            // Find all blocks where this value appears in uses
            let mut blocks_with_uses = BTreeSet::new();
            for &use_point in &live_range.uses {
                blocks_with_uses.insert(use_point.block);
            }

            // Mark live in blocks where it's used
            for &use_block_idx in &blocks_with_uses {
                if use_block_idx != live_range.def.block {
                    // Value is live from block entry (inst 0) to the last use in this block
                    let last_use_in_block = live_range
                        .uses
                        .iter()
                        .filter(|&&p| p.block == use_block_idx)
                        .map(|p| p.inst)
                        .max()
                        .unwrap_or(0);

                    for inst_idx in 0..=last_use_in_block {
                        let point = InstPoint {
                            block: use_block_idx,
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
    }

    LivenessInfo {
        live_ranges,
        live_sets,
        defs,
        uses,
        block_params,
    }
}

/// Format a function with liveness annotations as ASCII art.
///
/// This function displays the LPIR representation of a function with
/// liveness information shown as ASCII art on the right side. Each
/// row shows which variables are live at that instruction point.
pub fn format_function_with_liveness(func: &Function, liveness: &LivenessInfo) -> String {
    use alloc::format;

    // Find the maximum value index
    let mut max_value = 0u32;
    for value in liveness.live_ranges.keys() {
        max_value = max_value.max(value.index());
    }

    let mut output = String::new();

    // Format function signature
    if let Some(name) = &func.name {
        output.push_str(&format!("function %{}", name));
    } else {
        output.push_str("function");
    }

    // Signature parameters
    output.push('(');
    for (i, param_ty) in func.signature.params.iter().enumerate() {
        if i > 0 {
            output.push_str(", ");
        }
        output.push_str(&format!("{}", param_ty));
    }
    output.push(')');

    // Return type
    if !func.signature.returns.is_empty() {
        output.push_str(" -> ");
        for (i, ret_ty) in func.signature.returns.iter().enumerate() {
            if i > 0 {
                output.push_str(", ");
            }
            output.push_str(&format!("{}", ret_ty));
        }
    }

    output.push_str(" {\n\n");

    // Process each block
    for (block_idx, block) in func.blocks.iter().enumerate() {
        // Format block header
        output.push_str(&format!("block{}", block_idx));
        if !block.params.is_empty() {
            output.push('(');
            for (i, param) in block.params.iter().enumerate() {
                if i > 0 {
                    output.push_str(", ");
                }
                output.push_str(&format!("v{}: i32", param.index()));
            }
            output.push(')');
        }
        output.push_str(":\n");

        // Fixed column position for liveness separator
        // Header: 25 spaces + "; " (2 chars) = 27 chars before "00" starts
        const SEPARATOR_COLUMN: usize = 25;
        const MARKER_START_COLUMN: usize = 27; // Position where "00" starts in header

        // Print liveness header row
        output.push_str(&" ".repeat(SEPARATOR_COLUMN));
        output.push_str("; Liveness\n");
        output.push_str(&" ".repeat(SEPARATOR_COLUMN));
        output.push_str("; ");
        for i in 0..=max_value {
            output.push_str(&format!("{:02} ", i));
        }
        output.push('\n');

        // Process each instruction
        for (inst_idx, inst) in block.insts.iter().enumerate() {
            let point = InstPoint {
                block: block_idx,
                inst: inst_idx + 1, // +1 because 0 is reserved for block entry
            };

            // Format the instruction
            let inst_str = format!("    {}", inst);
            output.push_str(&inst_str);

            // Get live set at this point
            let live_set = liveness.live_sets.get(&point).cloned().unwrap_or_default();

            // Calculate padding to align markers with header column numbers
            // We want markers to start at MARKER_START_COLUMN (same as "00" in header)
            // So: instruction_len + padding + "; " (2 chars) should equal MARKER_START_COLUMN
            let current_pos = inst_str.len();
            let target_pos = MARKER_START_COLUMN;
            let padding = if current_pos + 2 < target_pos {
                target_pos - current_pos - 2 // Subtract 2 for "; "
            } else {
                0
            };
            output.push_str(&" ".repeat(padding));
            output.push_str("; ");

            // Create ASCII art row showing live variables
            // Each column is 3 characters wide (matching "00 " in header)
            for i in 0..=max_value {
                let value = Value::new(i);
                if live_set.contains(&value) {
                    output.push_str("■  ");
                } else {
                    output.push_str("   ");
                }
            }

            output.push('\n');
        }

        output.push('\n');
    }

    output.push_str("}\n");
    output
}

/// Debug function to print liveness visualization.
///
/// This function computes liveness for a function and prints it using the debug! macro.
/// It's intended for use in tests to visualize liveness information.
pub fn debug_liveness(func: &Function) {
    let liveness = compute_liveness(func);
    crate::debug!("{}", format_function_with_liveness(func, &liveness));
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
        debug_liveness(&func);
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
        debug_liveness(&func);
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
        debug_liveness(&func);
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
    brif v1, block1(v1), block1(v1)

block1(v2: i32):
    return v2
}"#;

        let func = parse_function(ir).expect("Failed to parse IR");
        debug_liveness(&func);
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
        // Test a value that is live across multiple blocks (explicitly passed)
        // Note: v1 is used at the brif instruction to pass to blocks 1 and 2
        // The block parameters v3 and v4 receive the value, but are different SSA values
        let ir = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 42
    v2 = iconst 1
    brif v2, block1(v1), block2(v1)

block1(v3: i32):
    return v3

block2(v4: i32):
    return v4
}"#;

        let func = parse_function(ir).expect("Failed to parse IR");
        debug_liveness(&func);
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
        // v1 is used at the brif instruction (block 0, inst 3) to pass to blocks 1 and 2
        // The last use is at the brif instruction where it's passed to the target blocks
        assert_eq!(
            v1_range.last_use.block, 0,
            "last use should be in block 0 at brif instruction, got block {}",
            v1_range.last_use.block
        );
        // Should have uses: at brif instruction (passed to both block1 and block2)
        assert!(!v1_range.uses.is_empty(), "v1 should have uses");
        // v1 is used at the brif instruction - it's passed to blocks but the actual
        // use in those blocks is via the block parameters (v3, v4) which are different values
    }

    #[test]
    fn test_liveness_loop() {
        // Test a value live across blocks (simplified loop pattern, explicitly passed)
        // Note: v1 is used at the jump instruction to pass to block1
        // The block parameter v2 receives the value, but is a different SSA value
        let ir = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 0
    jump block1(v1, v0)

block1(v2: i32, v3: i32):
    v4 = iadd v2, v3
    return v4
}"#;

        let func = parse_function(ir).expect("Failed to parse IR");
        debug_liveness(&func);
        let liveness = compute_liveness(&func);
        let v1 = Value::new(1);
        let v1_range = liveness.live_ranges.get(&v1).unwrap();

        // v1 should be defined in block 0
        assert_eq!(v1_range.def.block, 0);
        // v1 is used at the jump instruction (block 0) to pass to block1
        // The last use is at the jump instruction
        assert_eq!(v1_range.last_use.block, 0);
        // v1 is used at the jump instruction
        assert!(!v1_range.uses.is_empty());
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
        debug_liveness(&func);
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
        // Test value used in branch condition and passed to blocks
        let ir = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 1
    brif v0, block1(v1), block2(v1)

block1(v2: i32):
    return v2

block2(v3: i32):
    return v3
}"#;

        let func = parse_function(ir).expect("Failed to parse IR");
        debug_liveness(&func);
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
        debug_liveness(&func);
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
        debug_liveness(&func);
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
        debug_liveness(&func);
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
        debug_liveness(&func);
        let liveness = compute_liveness(&func);

        // v0 should be used in return
        let v0_range = liveness.live_ranges.get(&Value::new(0)).unwrap();
        assert_eq!(v0_range.def.block, 0);
        assert_eq!(v0_range.def.inst, 0); // Parameter
        assert!(!v0_range.uses.is_empty());
    }

    #[test]
    fn test_liveness_multiple_block_params() {
        // Test block with multiple parameters (explicitly passed)
        let ir = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 1
    v2 = iconst 2
    brif v1, block1(v2, v0), block1(v2, v0)

block1(v3: i32, v4: i32):
    v5 = iadd v3, v4
    return v5
}"#;

        let func = parse_function(ir).expect("Failed to parse IR");
        debug_liveness(&func);
        let liveness = compute_liveness(&func);

        // Block 1 should have two parameters
        assert!(liveness.block_params.contains_key(&(1, 0)));
        assert!(liveness.block_params.contains_key(&(1, 1)));

        let v3 = Value::new(3);
        let v4 = Value::new(4);
        assert_eq!(liveness.block_params.get(&(1, 0)), Some(&v3));
        assert_eq!(liveness.block_params.get(&(1, 1)), Some(&v4));
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
        debug_liveness(&func);
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
        debug_liveness(&func);
        let liveness = compute_liveness(&func);

        // v0 should be used in syscall and return
        let v0_range = liveness.live_ranges.get(&Value::new(0)).unwrap();
        assert!(v0_range.uses.len() >= 2);
    }

    #[test]
    fn test_liveness_complex_control_flow() {
        // Test complex nested control flow with explicit value passing
        // Note: block1 uses v2, so it needs v2 as a parameter
        let ir = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 1
    v2 = iconst 2
    brif v1, block1(v0, v2), block2(v0)

block1(v5: i32, v9: i32):
    brif v9, block3(v5), block4(v5)

block2(v6: i32):
    return v6

block3(v7: i32):
    return v7

block4(v8: i32):
    return v8
}"#;

        let func = parse_function(ir).expect("Failed to parse IR");
        debug_liveness(&func);
        let liveness = compute_liveness(&func);

        // v0 should be live across multiple blocks
        let v0_range = liveness.live_ranges.get(&Value::new(0)).unwrap();
        assert_eq!(v0_range.def.block, 0);
        // v0 is used at branches to pass to blocks 1, 2, 3, and 4
        // The last use is at the first brif instruction
        assert_eq!(v0_range.last_use.block, 0);
        assert!(!v0_range.uses.is_empty());

        // v1 should be used in first branch
        let v1_range = liveness.live_ranges.get(&Value::new(1)).unwrap();
        assert!(!v1_range.uses.is_empty());

        // v2 should be used in first branch (passed to block1) and in block1's branch
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
        debug_liveness(&func);
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
        debug_liveness(&func);
        let liveness = compute_liveness(&func);

        // v0 should be in defs at block entry
        let entry_point = InstPoint { block: 0, inst: 0 };
        let entry_defs = liveness.defs.get(&entry_point);
        assert!(entry_defs.is_some(), "Block entry should have definitions");
        assert!(
            entry_defs.unwrap().contains(&Value::new(0)),
            "v0 should be defined at block entry"
        );

        // v1 should be in defs at instruction 1
        let v1_def_point = InstPoint { block: 0, inst: 1 };
        let v1_defs = liveness.defs.get(&v1_def_point);
        assert!(v1_defs.is_some(), "Instruction 1 should have definitions");
        assert!(
            v1_defs.unwrap().contains(&Value::new(1)),
            "v1 should be defined at instruction 1"
        );

        // v2 should be in defs at instruction 2
        let v2_def_point = InstPoint { block: 0, inst: 2 };
        let v2_defs = liveness.defs.get(&v2_def_point);
        assert!(v2_defs.is_some(), "Instruction 2 should have definitions");
        assert!(
            v2_defs.unwrap().contains(&Value::new(2)),
            "v2 should be defined at instruction 2"
        );

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
        debug_liveness(&func);
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
        // Test jump instruction with explicit value passing
        // Note: v1 is used at the jump instruction to pass to block1
        // The block parameter v2 receives the value, but is a different SSA value
        let ir = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 42
    jump block1(v1)

block1(v2: i32):
    return v2
}"#;

        let func = parse_function(ir).expect("Failed to parse IR");
        debug_liveness(&func);
        let liveness = compute_liveness(&func);

        // v1 should be defined in block 0
        let v1_range = liveness.live_ranges.get(&Value::new(1)).unwrap();
        assert_eq!(v1_range.def.block, 0);
        // v1 is used at the jump instruction (block 0) to pass to block1
        // The last use is at the jump instruction
        assert_eq!(v1_range.last_use.block, 0);
        // v1 is used at the jump instruction
        assert!(!v1_range.uses.is_empty());
    }

    #[test]
    fn test_liveness_visualization_10_params() {
        extern crate std;

        // Test the 10-parameter sum function with liveness visualization
        let ir = r#"
function %test(i32, i32, i32, i32, i32, i32, i32, i32, i32, i32) -> i32 {
block0(v0: i32, v1: i32, v2: i32, v3: i32, v4: i32, v5: i32, v6: i32, v7: i32, v8: i32, v9: i32):
    ; Sum all arguments
    v10 = iadd v0, v1
    v11 = iadd v10, v2
    v12 = iadd v11, v3
    v13 = iadd v12, v4
    v14 = iadd v13, v5
    v15 = iadd v14, v6
    v16 = iadd v15, v7
    v17 = iadd v16, v8
    v18 = iadd v17, v9
    return v18
}
"#;

        let func = parse_function(ir).expect("Failed to parse IR");
        debug_liveness(&func);
        let liveness = compute_liveness(&func);

        // Verify liveness for specific instructions
        // At v10 = iadd v0, v1: v0 and v1 are used at this instruction
        let point_v10 = InstPoint { block: 0, inst: 1 };
        let uses_at_v10 = liveness.uses.get(&point_v10);
        assert!(uses_at_v10.is_some(), "v10 instruction should have uses");
        let uses = uses_at_v10.unwrap();
        assert!(
            uses.contains(&Value::new(0)),
            "v0 should be used at v10 instruction"
        );
        assert!(
            uses.contains(&Value::new(1)),
            "v1 should be used at v10 instruction"
        );

        // At v11 = iadd v10, v2: v10 and v2 are used at this instruction
        let point_v11 = InstPoint { block: 0, inst: 2 };
        let uses_at_v11 = liveness.uses.get(&point_v11);
        assert!(uses_at_v11.is_some(), "v11 instruction should have uses");
        let uses_v11 = uses_at_v11.unwrap();
        assert!(
            uses_v11.contains(&Value::new(10)),
            "v10 should be used at v11 instruction"
        );
        assert!(
            uses_v11.contains(&Value::new(2)),
            "v2 should be used at v11 instruction"
        );

        // At return v18: v18 is used at this instruction
        let point_return = InstPoint { block: 0, inst: 10 };
        let uses_at_return = liveness.uses.get(&point_return);
        assert!(
            uses_at_return.is_some(),
            "return instruction should have uses"
        );
        let uses_return = uses_at_return.unwrap();
        assert!(
            uses_return.contains(&Value::new(18)),
            "v18 should be used at return"
        );

        // Verify that instruction result values have live ranges
        for i in 10..=18 {
            let value = Value::new(i);
            assert!(
                liveness.live_ranges.contains_key(&value),
                "Value v{} should have a live range",
                i
            );
        }

        // Verify that parameters are tracked in defs at block entry
        let entry_point = InstPoint { block: 0, inst: 0 };
        // At least some parameters should be in defs (checking a few)
        assert!(
            liveness.defs.contains_key(&entry_point),
            "Block entry should have parameter definitions"
        );

        // Print visualization for manual inspection (useful during development)
        // Uncomment to see the output:
        // println!("{}", visualization);
    }
}
