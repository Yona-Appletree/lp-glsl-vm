//! Data Flow Graph (instruction data).

use crate::{entity::Inst as InstEntity, entity_map::PrimaryMap, Type, Value};

pub mod inst_data;
pub mod opcode;

pub use inst_data::{BlockArgs, Immediate, InstData};
pub use opcode::Opcode;

/// Data Flow Graph - stores instruction data
///
/// The DFG stores what instructions do (opcode + operands), separate
/// from where they appear in the layout. This separation enables
/// efficient optimizations and transformations.
#[derive(Debug, Clone)]
pub struct DFG {
    /// Instruction data
    pub insts: PrimaryMap<InstEntity, InstData>,
    /// Value types (for type checking)
    pub value_types: PrimaryMap<Value, Type>,
}

impl DFG {
    /// Create a new empty DFG
    pub fn new() -> Self {
        Self {
            insts: PrimaryMap::new(),
            value_types: PrimaryMap::new(),
        }
    }

    /// Create an instruction and return its entity
    ///
    /// The instruction is added to the DFG but not yet inserted into
    /// the layout. Use Layout::append_inst() or Layout::insert_inst()
    /// to add it to a block.
    pub fn create_inst(&mut self, data: InstData) -> InstEntity {
        // Create results and assign types if needed
        for result in &data.results {
            // Type inference: determine type from opcode
            let ty = self.infer_result_type(&data.opcode, &data.ty);
            if let Some(ty) = ty {
                // Only set if not already set (allows explicit type setting)
                if self.value_types.get(*result).is_none() {
                    self.set_value_type(*result, ty);
                }
            }
        }

        self.insts.push(data)
    }

    /// Get instruction data
    pub fn inst_data(&self, inst: InstEntity) -> Option<&InstData> {
        self.insts.get(inst)
    }

    /// Get mutable instruction data
    pub fn inst_data_mut(&mut self, inst: InstEntity) -> Option<&mut InstData> {
        self.insts.get_mut(inst)
    }

    /// Get instruction arguments
    pub fn inst_args(&self, inst: InstEntity) -> &[Value] {
        self.inst_data(inst)
            .map(|data| data.args.as_slice())
            .unwrap_or(&[])
    }

    /// Get instruction results
    pub fn inst_results(&self, inst: InstEntity) -> &[Value] {
        self.inst_data(inst)
            .map(|data| data.results.as_slice())
            .unwrap_or(&[])
    }

    /// Get the type of a value
    pub fn value_type(&self, value: Value) -> Option<Type> {
        self.value_types.get(value).copied()
    }

    /// Set the type of a value
    pub fn set_value_type(&mut self, value: Value, ty: Type) {
        let index = value.index() as usize;
        // Ensure the value exists in the map by growing if necessary
        if index >= self.value_types.len() {
            // Reserve capacity to avoid multiple reallocations
            let needed = index + 1;
            self.value_types
                .reserve(needed.saturating_sub(self.value_types.capacity()));
            // Push the actual value directly until we reach the target index (inclusive)
            while self.value_types.len() <= index {
                self.value_types.push(ty);
            }
        } else {
            // Value already exists, update it
            if let Some(existing_ty) = self.value_types.get_mut(value) {
                *existing_ty = ty;
            }
        }
    }

    /// Get the next available value index
    ///
    /// This computes the next value index by finding the maximum value index
    /// currently in use. This is used by builders to allocate new values.
    pub fn next_value_index(&self) -> u32 {
        // Find the maximum value index in use
        // value_types is a PrimaryMap, so we can iterate over it
        let max_index = self
            .value_types
            .iter()
            .map(|(value, _)| value.index() as u32)
            .max()
            .unwrap_or(0);
        max_index + 1
    }

    /// Infer the result type from an opcode
    ///
    /// This is a helper for type inference. Returns None if the type
    /// cannot be inferred or must be specified explicitly (e.g., Load).
    fn infer_result_type(&self, opcode: &Opcode, explicit_ty: &Option<Type>) -> Option<Type> {
        // If explicit type is provided (e.g., for Load), use it
        if let Some(ty) = explicit_ty {
            return Some(*ty);
        }

        match opcode {
            Opcode::Iadd
            | Opcode::Isub
            | Opcode::Imul
            | Opcode::Idiv
            | Opcode::Irem
            | Opcode::Iconst => Some(Type::I32),
            Opcode::Icmp { .. } => Some(Type::I32), // Integer comparisons return i32 (0/1)
            Opcode::Fcmp { .. } => Some(Type::I32), // Floating point comparisons return i32 (0/1)
            Opcode::Fconst => Some(Type::F32),
            Opcode::Load => None, // Must be specified explicitly
            Opcode::Store
            | Opcode::Jump
            | Opcode::Br
            | Opcode::Return
            | Opcode::Halt
            | Opcode::Syscall
            | Opcode::Trap { .. }
            | Opcode::Trapz { .. }
            | Opcode::Trapnz { .. } => None, // No results
            Opcode::Call { .. } => None, // Call results depend on function signature
        }
    }
}

impl Default for DFG {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use crate::dfg::opcode::Opcode;

    #[test]
    fn test_dfg_new() {
        let dfg = DFG::new();
        assert_eq!(dfg.insts.len(), 0);
        assert_eq!(dfg.value_types.len(), 0);
    }

    #[test]
    fn test_dfg_create_inst() {
        let mut dfg = DFG::new();
        let v1 = Value::new(1);
        let v2 = Value::new(2);
        let v3 = Value::new(3);
        let data = InstData::arithmetic(Opcode::Iadd, v3, v1, v2);

        let inst = dfg.create_inst(data);

        assert_eq!(dfg.insts.len(), 1);
        let inst_data = dfg.inst_data(inst).unwrap();
        assert_eq!(inst_data.opcode, Opcode::Iadd);
    }

    #[test]
    fn test_dfg_inst_data() {
        let mut dfg = DFG::new();
        let v1 = Value::new(1);
        let v2 = Value::new(2);
        let v3 = Value::new(3);
        let data = InstData::arithmetic(Opcode::Iadd, v3, v1, v2);

        let inst = dfg.create_inst(data);
        let retrieved = dfg.inst_data(inst).unwrap();

        assert_eq!(retrieved.opcode, Opcode::Iadd);
        assert_eq!(retrieved.args, vec![v1, v2]);
        assert_eq!(retrieved.results, vec![v3]);
    }

    #[test]
    fn test_dfg_inst_args_results() {
        let mut dfg = DFG::new();
        let v1 = Value::new(1);
        let v2 = Value::new(2);
        let v3 = Value::new(3);
        let data = InstData::arithmetic(Opcode::Iadd, v3, v1, v2);

        let inst = dfg.create_inst(data);

        assert_eq!(dfg.inst_args(inst), &[v1, v2]);
        assert_eq!(dfg.inst_results(inst), &[v3]);
    }

    #[test]
    fn test_dfg_value_type() {
        let mut dfg = DFG::new();
        let v1 = Value::new(1);
        let v2 = Value::new(2);
        let v3 = Value::new(3);
        let data = InstData::arithmetic(Opcode::Iadd, v3, v1, v2);

        let _inst = dfg.create_inst(data);

        // Type should be inferred for result
        assert_eq!(dfg.value_type(v3), Some(Type::I32));
    }

    #[test]
    fn test_dfg_set_value_type() {
        let mut dfg = DFG::new();
        let v1 = Value::new(0);

        dfg.set_value_type(v1, Type::F32);
        assert_eq!(dfg.value_type(v1), Some(Type::F32));
    }
}
