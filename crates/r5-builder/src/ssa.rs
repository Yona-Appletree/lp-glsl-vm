//! SSA construction helpers.

use alloc::{collections::BTreeMap, string::String, vec::Vec};

use r5_ir::Value;

/// Tracks variable definitions for SSA construction.
///
/// In SSA form, each variable can have multiple definitions (one per path).
/// This builder tracks the current definition of each variable in each block.
#[derive(Debug, Clone)]
pub struct SSABuilder {
    /// Map from variable name to the current value for that variable.
    /// Each variable can have different values in different blocks.
    variable_values: BTreeMap<String, Vec<(u32, Value)>>, // (block_id, value)
    /// Next available value index.
    next_value: u32,
}

impl SSABuilder {
    /// Create a new SSA builder.
    pub fn new() -> Self {
        Self {
            variable_values: BTreeMap::new(),
            next_value: 0,
        }
    }

    /// Allocate a new value.
    pub fn new_value(&mut self) -> Value {
        let value = Value::new(self.next_value);
        self.next_value += 1;
        value
    }

    /// Define a variable in the current block.
    ///
    /// This creates a new value and records that the variable now has this value
    /// in the given block.
    pub fn define_var(&mut self, var_name: &str, block_id: u32, value: Value) {
        self.variable_values
            .entry(String::from(var_name))
            .or_insert_with(Vec::new)
            .push((block_id, value));
    }

    /// Get the current value of a variable in a given block.
    ///
    /// Returns the most recent definition of the variable in this block or
    /// any predecessor block.
    ///
    /// For now, this is simplified - in a full implementation, we'd need
    /// to track the CFG and find the value from the appropriate predecessor.
    pub fn use_var(&self, var_name: &str, block_id: u32) -> Option<Value> {
        self.variable_values
            .get(var_name)?
            .iter()
            .rev()
            .find(|(bid, _)| *bid <= block_id)
            .map(|(_, v)| *v)
    }

    /// Clear all variable definitions (for starting a new function).
    pub fn clear(&mut self) {
        self.variable_values.clear();
        self.next_value = 0;
    }

    /// Get the next value index (for debugging).
    pub fn next_value_index(&self) -> u32 {
        self.next_value
    }
}

impl Default for SSABuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_value() {
        let mut ssa = SSABuilder::new();
        let v1 = ssa.new_value();
        let v2 = ssa.new_value();
        assert_eq!(v1.index(), 0);
        assert_eq!(v2.index(), 1);
    }

    #[test]
    fn test_define_and_use_var() {
        let mut ssa = SSABuilder::new();
        let v1 = ssa.new_value();
        ssa.define_var("x", 0, v1);
        assert_eq!(ssa.use_var("x", 0), Some(v1));
    }

    #[test]
    fn test_use_undefined_var() {
        let ssa = SSABuilder::new();
        assert_eq!(ssa.use_var("x", 0), None);
    }
}
