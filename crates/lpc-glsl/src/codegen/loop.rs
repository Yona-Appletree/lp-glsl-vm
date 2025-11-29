//! Loop tracking for GLSL codegen.
//!
//! This module provides tracking of loop information for proper break/continue handling.

use alloc::{collections::BTreeSet, string::String, vec::Vec};

use lpc_lpir::BlockEntity;

/// Information about a loop.
pub struct LoopInfo {
    /// Loop header block (where condition is checked)
    header: BlockEntity,
    /// Loop exit block (where we go when condition is false)
    exit: BlockEntity,
    /// Continue block (where we go on continue) - optional
    continue_block: Option<BlockEntity>,
    /// Variables modified in this loop (need phi nodes)
    modified_vars: BTreeSet<String>,
    /// Variables used in loop condition
    cond_vars: BTreeSet<String>,
}

impl LoopInfo {
    /// Create a new loop info.
    pub fn new(header: BlockEntity, exit: BlockEntity) -> Self {
        Self {
            header,
            exit,
            continue_block: None,
            modified_vars: BTreeSet::new(),
            cond_vars: BTreeSet::new(),
        }
    }

    /// Set the continue block.
    pub fn set_continue_block(&mut self, block: BlockEntity) {
        self.continue_block = Some(block);
    }

    /// Get the header block.
    pub fn header(&self) -> BlockEntity {
        self.header
    }

    /// Get the exit block.
    pub fn exit(&self) -> BlockEntity {
        self.exit
    }

    /// Get the continue block.
    pub fn continue_block(&self) -> Option<BlockEntity> {
        self.continue_block
    }

    /// Mark a variable as modified in this loop.
    pub fn mark_modified(&mut self, var: String) {
        self.modified_vars.insert(var);
    }

    /// Mark a variable as used in the loop condition.
    pub fn mark_cond_var(&mut self, var: String) {
        self.cond_vars.insert(var);
    }

    /// Get variables modified in this loop.
    pub fn modified_vars(&self) -> &BTreeSet<String> {
        &self.modified_vars
    }

    /// Get variables used in loop condition.
    pub fn cond_vars(&self) -> &BTreeSet<String> {
        &self.cond_vars
    }
}

/// Stack of nested loops.
pub struct LoopStack {
    loops: Vec<LoopInfo>,
}

impl LoopStack {
    /// Create a new empty loop stack.
    pub fn new() -> Self {
        Self { loops: Vec::new() }
    }

    /// Push a loop onto the stack.
    pub fn push(&mut self, info: LoopInfo) {
        self.loops.push(info);
    }

    /// Pop a loop from the stack.
    pub fn pop(&mut self) -> Option<LoopInfo> {
        self.loops.pop()
    }

    /// Get the current loop (if any).
    pub fn current(&self) -> Option<&LoopInfo> {
        self.loops.last()
    }

    /// Get the current loop mutably (if any).
    pub fn current_mut(&mut self) -> Option<&mut LoopInfo> {
        self.loops.last_mut()
    }

    /// Find the target block for a break statement.
    ///
    /// Returns the exit block of the innermost loop.
    pub fn find_break_target(&self) -> Option<BlockEntity> {
        self.loops.last().map(|info| info.exit())
    }

    /// Find the target block for a continue statement.
    ///
    /// Returns the continue block (or header if no continue block) of the innermost loop.
    pub fn find_continue_target(&self) -> Option<BlockEntity> {
        self.loops
            .last()
            .and_then(|info| info.continue_block().or(Some(info.header())))
    }

    /// Check if we're currently inside a loop.
    pub fn is_in_loop(&self) -> bool {
        !self.loops.is_empty()
    }
}
