//! Branch resolution: Convert two-dest branches to single-dest branches
//!
//! This module provides ISA-agnostic branch resolution functionality that works
//! with any ISA through the MachInst trait. It handles:
//!
//! - Two-dest to single-dest branch conversion
//! - Fallthrough detection based on block emission order
//! - Branch optimization (basic)

use crate::backend3::types::BlockIndex;

/// Branch information for emission
#[derive(Debug, Clone)]
pub enum BranchInfo {
    /// Two-destination branch (conditional)
    TwoDest {
        target_true: BlockIndex,
        target_false: BlockIndex,
    },
    /// Single-destination branch (unconditional)
    OneDest { target: BlockIndex },
}

/// Determine which branch target is the fallthrough based on block order
///
/// Returns `Some((target_block, invert))` where:
/// - `target_block` is the block to branch to
/// - `invert` indicates whether the condition should be inverted
///
/// Returns `None` if neither target is the fallthrough.
pub fn determine_fallthrough(
    current_block: BlockIndex,
    target_true: BlockIndex,
    target_false: BlockIndex,
    emission_order: &[BlockIndex],
) -> Option<(BlockIndex, bool)> {
    // Find current block's position in emission order
    let current_pos = emission_order.iter().position(|&b| b == current_block)?;

    // Check if next block is one of our targets
    if let Some(&next_block) = emission_order.get(current_pos + 1) {
        if next_block == target_false {
            // False is fallthrough, branch to true
            return Some((target_true, false));
        } else if next_block == target_true {
            // True is fallthrough, branch to false (invert condition)
            return Some((target_false, true));
        }
    }

    // Neither is fallthrough - return None to indicate no fallthrough
    None
}

/// Resolve two-dest branch to single-dest branch
///
/// This converts a two-dest branch (conditional with two targets) to a
/// single-dest branch based on fallthrough detection. Returns the target
/// block and whether the condition should be inverted.
pub fn resolve_two_dest_branch(
    current_block: BlockIndex,
    target_true: BlockIndex,
    target_false: BlockIndex,
    emission_order: &[BlockIndex],
) -> (BlockIndex, bool) {
    determine_fallthrough(current_block, target_true, target_false, emission_order).unwrap_or_else(
        || {
            // Fallback: if we can't determine fallthrough, branch to true
            (target_true, false)
        },
    )
}
