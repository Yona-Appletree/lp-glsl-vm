//! Liveness analysis for register allocation.
//!
//! This module computes which values are live at each point in a function,
//! enabling register allocation to make informed decisions about when
//! values can be spilled or registers can be reused.

use alloc::{
    collections::{BTreeMap, BTreeSet},
    vec::Vec,
};

use r5_ir::{Function, Inst, Value};

/// Instruction point (block index, instruction index).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InstPoint {
    pub block: usize,
    pub inst: usize,
}

impl InstPoint {
    pub fn new(block: usize, inst: usize) -> Self {
        Self { block, inst }
    }
}

/// Live range for a value (from definition to last use).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveRange {
    /// Point where the value is defined
    pub def: InstPoint,
    /// Point where the value is last used
    pub last_use: InstPoint,
    /// All use points for this value
    pub uses: Vec<InstPoint>,
}

impl LiveRange {
    pub fn new(def: InstPoint) -> Self {
        Self {
            def,
            last_use: def,
            uses: Vec::new(),
        }
    }

    pub fn add_use(&mut self, use_point: InstPoint) {
        if use_point > self.last_use {
            self.last_use = use_point;
        }
        self.uses.push(use_point);
    }

    /// Check if this live range overlaps with another at a given point.
    pub fn overlaps_at(&self, other: &LiveRange, point: InstPoint) -> bool {
        // Both must be defined before or at this point
        if self.def > point || other.def > point {
            return false;
        }
        // Both must be used at or after this point
        if self.last_use < point || other.last_use < point {
            return false;
        }
        true
    }
}

/// Liveness information for a function.
#[derive(Debug, Clone)]
pub struct LivenessInfo {
    /// Live range for each value
    pub live_ranges: BTreeMap<Value, LiveRange>,
    /// Set of live values at each instruction point
    pub live_sets: BTreeMap<InstPoint, BTreeSet<Value>>,
    /// Values defined at each instruction point
    pub defs: BTreeMap<InstPoint, Value>,
    /// Values used at each instruction point
    pub uses: BTreeMap<InstPoint, Vec<Value>>,
}

impl LivenessInfo {
    /// Get the live set at a given instruction point.
    pub fn live_at(&self, point: InstPoint) -> BTreeSet<Value> {
        self.live_sets.get(&point).cloned().unwrap_or_default()
    }

    /// Check if a value is live at a given point.
    pub fn is_live(&self, value: Value, point: InstPoint) -> bool {
        self.live_sets
            .get(&point)
            .map(|set| set.contains(&value))
            .unwrap_or(false)
    }

    /// Get the live range for a value.
    pub fn live_range(&self, value: Value) -> Option<&LiveRange> {
        self.live_ranges.get(&value)
    }
}

/// Compute liveness for all values in a function.
pub fn compute_liveness(func: &Function) -> LivenessInfo {
    let mut defs = BTreeMap::new();
    let mut uses = BTreeMap::new();
    let mut live_ranges = BTreeMap::new();

    // Step 1: Forward pass - collect all definitions and uses
    for (block_idx, block) in func.blocks.iter().enumerate() {
        // Handle block parameters (they're defined at block entry)
        let block_entry = InstPoint::new(block_idx, 0);
        for param in &block.params {
            // Block parameters are "defined" at block entry
            // We'll track them separately, but for now mark them as defined
            defs.insert(block_entry, *param);
            // Initialize live range for block parameters
            live_ranges.insert(*param, LiveRange::new(block_entry));
        }

        // Process instructions in this block
        for (inst_idx, inst) in block.insts.iter().enumerate() {
            let point = InstPoint::new(block_idx, inst_idx + 1); // +1 because 0 is block entry

            // Collect definitions (results)
            // Note: Return instructions don't define new values, they just use existing ones
            for result in inst.results() {
                // Only create new live range if value isn't already defined
                if !live_ranges.contains_key(&result) {
                    defs.insert(point, result);
                    live_ranges.insert(result, LiveRange::new(point));
                }
            }

            // Collect uses (arguments)
            let inst_uses = inst.args();
            if !inst_uses.is_empty() {
                uses.insert(point, inst_uses.clone());
            }

            // Update live ranges for used values
            for used_value in &inst_uses {
                if let Some(live_range) = live_ranges.get_mut(used_value) {
                    live_range.add_use(point);
                } else {
                    // Value used before definition (block parameter or from another block)
                    // Create live range starting from block entry
                    let block_entry = InstPoint::new(block_idx, 0);
                    let mut live_range = LiveRange::new(block_entry);
                    live_range.add_use(point);
                    live_ranges.insert(*used_value, live_range);
                }
            }
        }
    }

    // Step 2: Return values are already handled in step 1 via inst.args()
    // No need to process them again

    // Step 3: Handle values used in successor blocks (for block parameters)
    // For each block, find its predecessors and mark their values as live
    for (block_idx, block) in func.blocks.iter().enumerate() {
        let block_entry = InstPoint::new(block_idx, 0);

        // Find predecessors by looking for jumps/branches to this block
        for (_pred_block_idx, pred_block) in func.blocks.iter().enumerate() {
            for inst in &pred_block.insts {
                let targets_this_block = match inst {
                    Inst::Jump { target } => *target as usize == block_idx,
                    Inst::Br {
                        target_true,
                        target_false,
                        ..
                    } => *target_true as usize == block_idx || *target_false as usize == block_idx,
                    _ => false,
                };

                if targets_this_block {
                    // Values used in block parameters are live at the end of predecessor
                    // For now, we'll mark block parameters as live from block entry
                    for param in &block.params {
                        if let Some(live_range) = live_ranges.get_mut(param) {
                            // Ensure block entry is the def point
                            live_range.def = block_entry;
                        }
                    }
                }
            }
        }
    }

    // Step 4: Build live sets for each instruction point
    let mut live_sets = BTreeMap::new();

    // For each instruction point, compute which values are live
    for (block_idx, block) in func.blocks.iter().enumerate() {
        // Block entry point
        let block_entry = InstPoint::new(block_idx, 0);
        let mut live_set = BTreeSet::new();
        // Block parameters are live at entry
        for param in &block.params {
            live_set.insert(*param);
        }
        live_sets.insert(block_entry, live_set);

        // Process instructions
        for (inst_idx, inst) in block.insts.iter().enumerate() {
            let point = InstPoint::new(block_idx, inst_idx + 1);

            // Start with live set from previous point (or block entry for first inst)
            let prev_point = if inst_idx == 0 {
                block_entry
            } else {
                InstPoint::new(block_idx, inst_idx)
            };
            let mut live_set = live_sets.get(&prev_point).cloned().unwrap_or_default();

            // Add values that are used here (they become live)
            for used_value in inst.args() {
                live_set.insert(used_value);
            }

            // Handle return - values in return are used
            if let Inst::Return { values } = inst {
                for value in values {
                    live_set.insert(*value);
                }
            }

            // Remove values that are defined here (killed)
            // Do this AFTER adding uses, so values used and defined in same inst are handled correctly
            for result in inst.results() {
                live_set.remove(&result);
            }

            live_sets.insert(point, live_set);
        }
    }

    LivenessInfo {
        live_ranges,
        live_sets,
        defs,
        uses,
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use r5_ir::{parse_function, Block, Function, Signature, Type};

    use super::*;

    #[test]
    fn test_liveness_simple_sequential() {
        // Simple function: v0 = iconst 1; v1 = iconst 2; v2 = iadd v0, v1; return v2
        let sig = Signature::new(vec![], vec![Type::I32]);
        let mut func = Function::new(sig);

        let mut block = Block::new();
        let v0 = Value::new(0);
        let v1 = Value::new(1);
        let v2 = Value::new(2);

        block.push_inst(Inst::Iconst {
            result: v0,
            value: 1,
        });
        block.push_inst(Inst::Iconst {
            result: v1,
            value: 2,
        });
        block.push_inst(Inst::Iadd {
            result: v2,
            arg1: v0,
            arg2: v1,
        });
        block.push_inst(Inst::Return { values: vec![v2] });

        func.add_block(block);

        let liveness = compute_liveness(&func);

        // v0 should be live from def (inst 1) to last use (inst 3)
        let v0_range = liveness.live_range(v0).unwrap();
        assert_eq!(v0_range.def, InstPoint::new(0, 1));
        assert_eq!(v0_range.last_use, InstPoint::new(0, 3));
        assert!(v0_range.uses.contains(&InstPoint::new(0, 3)));

        // v1 should be live from def (inst 2) to last use (inst 3)
        let v1_range = liveness.live_range(v1).unwrap();
        assert_eq!(v1_range.def, InstPoint::new(0, 2));
        assert!(v1_range.uses.contains(&InstPoint::new(0, 3)));
        assert_eq!(v1_range.last_use, InstPoint::new(0, 3));

        // v2 should be live from def (inst 3) to last use (inst 4)
        let v2_range = liveness.live_range(v2).unwrap();
        assert_eq!(v2_range.def, InstPoint::new(0, 3));
        assert!(v2_range.uses.contains(&InstPoint::new(0, 4)));
        assert_eq!(v2_range.last_use, InstPoint::new(0, 4));
    }

    #[test]
    fn test_liveness_block_parameters() {
        // Function with block parameters
        let sig = Signature::new(vec![Type::I32], vec![Type::I32]);
        let mut func = Function::new(sig);

        let mut block0 = Block::new();
        let param = Value::new(0);
        block0.params.push(param);
        let v1 = Value::new(1);
        block0.push_inst(Inst::Iadd {
            result: v1,
            arg1: param,
            arg2: param,
        });
        block0.push_inst(Inst::Return { values: vec![v1] });
        func.add_block(block0);

        let liveness = compute_liveness(&func);

        // param should be live from block entry (0, 0) to use (0, 1)
        let param_range = liveness.live_range(param).unwrap();
        assert_eq!(param_range.def, InstPoint::new(0, 0));
        assert!(param_range.uses.contains(&InstPoint::new(0, 1)));
    }

    #[test]
    fn test_liveness_unused_values() {
        // Function with unused value
        let sig = Signature::empty();
        let mut func = Function::new(sig);

        let mut block = Block::new();
        let v0 = Value::new(0);
        let v1 = Value::new(1);

        block.push_inst(Inst::Iconst {
            result: v0,
            value: 1,
        });
        block.push_inst(Inst::Iconst {
            result: v1,
            value: 2,
        });
        block.push_inst(Inst::Return { values: vec![v1] }); // v0 unused

        func.add_block(block);

        let liveness = compute_liveness(&func);

        // v0 should have a live range but last_use == def (unused)
        let v0_range = liveness.live_range(v0).unwrap();
        assert_eq!(v0_range.def, v0_range.last_use);
    }

    #[test]
    fn test_liveness_multiple_uses() {
        // Value used multiple times
        let sig = Signature::empty();
        let mut func = Function::new(sig);

        let mut block = Block::new();
        let v0 = Value::new(0);
        let v1 = Value::new(1);
        let v2 = Value::new(2);
        let _v3 = Value::new(3);

        block.push_inst(Inst::Iconst {
            result: v0,
            value: 1,
        });
        block.push_inst(Inst::Iadd {
            result: v1,
            arg1: v0,
            arg2: v0, // v0 used twice
        });
        block.push_inst(Inst::Iadd {
            result: v2,
            arg1: v1,
            arg2: v0, // v0 used again
        });
        block.push_inst(Inst::Return { values: vec![v2] });

        func.add_block(block);

        let liveness = compute_liveness(&func);

        // v0 should be live from def (inst 1) to last use (inst 3)
        let v0_range = liveness.live_range(v0).unwrap();
        assert_eq!(v0_range.def, InstPoint::new(0, 1));
        assert_eq!(v0_range.last_use, InstPoint::new(0, 3));
        assert_eq!(v0_range.uses.len(), 3); // Used in inst 2, 2, and 3
    }

    #[test]
    fn test_liveness_across_calls() {
        // Function with a call - values live across calls
        let ir = r#"
function %test() -> i32 {
block0:
    v0 = iconst 10
    v1 = iconst 20
    call %helper(v0) -> v2
    v3 = iadd v1, v2
    return v3
}"#;

        let func = parse_function(ir).expect("Failed to parse IR function");
        let liveness = compute_liveness(&func);

        // v1 should be live across the call
        let v1 = r5_ir::Value::new(1);
        if let Some(v1_range) = liveness.live_range(v1) {
            // v1 is defined before call and used after
            assert!(v1_range.def.inst < 3); // Before call
            assert!(v1_range.last_use.inst >= 3); // After call
        }
    }

    #[test]
    fn test_liveness_loop() {
        // Function with a loop (jump back)
        let ir = r#"
function %test() -> i32 {
block0:
    v0 = iconst 0
    jump block1
block1:
    v1 = iadd v0, v0
    jump block1
}"#;

        let func = parse_function(ir).expect("Failed to parse IR function");
        let liveness = compute_liveness(&func);

        // v0 should be live in block1 (used in loop)
        let v0 = r5_ir::Value::new(0);
        if let Some(v0_range) = liveness.live_range(v0) {
            // Should have uses in block1
            assert!(!v0_range.uses.is_empty());
        }
    }

    #[test]
    fn test_liveness_conditional() {
        // Function with conditional branch
        let ir = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 0
    brif v0, block1, block2
block1:
    v2 = iadd v0, v1
    return v2
block2:
    v3 = isub v0, v1
    return v3
}"#;

        let func = parse_function(ir).expect("Failed to parse IR function");
        let liveness = compute_liveness(&func);

        // v0 should be live in both block1 and block2
        let v0 = r5_ir::Value::new(0);
        if let Some(v0_range) = liveness.live_range(v0) {
            // Should have multiple uses (in block1 and block2)
            assert!(v0_range.uses.len() >= 2);
        }
    }

    #[test]
    fn test_liveness_block_params() {
        // Function with block parameters (phi-like)
        let ir = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    jump block1
block1(v1: i32):
    v2 = iadd v1, v0
    return v2
}"#;

        let func = parse_function(ir).expect("Failed to parse IR function");
        let liveness = compute_liveness(&func);

        // Block parameter v1 should be live from block entry
        let v1 = r5_ir::Value::new(1);
        if let Some(v1_range) = liveness.live_range(v1) {
            assert_eq!(v1_range.def, InstPoint::new(1, 0)); // Block entry
        }
    }

    #[test]
    fn test_liveness_long_chain() {
        // Long chain of values to test live range tracking
        let ir = r#"
function %test() -> i32 {
block0:
    v0 = iconst 1
    v1 = iconst 2
    v2 = iconst 3
    v3 = iconst 4
    v4 = iconst 5
    v5 = iadd v0, v1
    v6 = iadd v2, v3
    v7 = iadd v4, v5
    v8 = iadd v6, v7
    return v8
}"#;

        let func = parse_function(ir).expect("Failed to parse IR function");
        let liveness = compute_liveness(&func);

        // All values should have live ranges
        for i in 0..=8 {
            let value = r5_ir::Value::new(i);
            assert!(
                liveness.live_range(value).is_some(),
                "Value v{} should have a live range",
                i
            );
        }

        // v8 should be live until return
        let v8 = r5_ir::Value::new(8);
        let v8_range = liveness.live_range(v8).unwrap();
        // Return instruction is at inst 9 (0-indexed block entry + 9 instructions)
        // But we use 1-indexed for instructions, so it's InstPoint::new(0, 9)
        // Check that v8 is used at or before the return
        assert!(
            v8_range.last_use.inst >= 9,
            "v8 should be live until return"
        );
    }
}
