//! Instruction lowering (IR → RISC-V).
//!
//! This module lowers IR instructions to RISC-V instructions using pre-computed
//! register allocation, spill/reload plan, and frame layout.

mod arithmetic;
mod branch;
mod call;
mod comparisons;
mod epilogue;
mod function;
mod helpers;
mod iconst;
mod prologue;
mod return_;
mod syscall;
mod types;

use alloc::{collections::BTreeMap, string::String, vec::Vec};

use lpc_lpir::Inst;
// Re-export public types
pub use types::{
    ByteOffset, ByteSize, InstOffset, InstSize, LoweringError, Relocation, RelocationInstType,
    RelocationTarget, WordSize,
};

use super::{abi::AbiInfo, frame::FrameLayout, regalloc::RegisterAllocation};
use crate::Inst as RiscvInst;
use crate::inst_buffer::InstBuffer;

/// Lower IR to RISC-V 32-bit code.
///
/// Uses pre-computed register allocation, spill/reload plan, and frame layout.
pub struct Lowerer {
    /// Module context for function calls (optional).
    module: Option<lpc_lpir::Module>,
    /// Function addresses (for call relocations).
    function_addresses: BTreeMap<String, u32>,
    /// Relocations that need to be fixed up (call sites, for module-level fixup).
    pub(super) relocations: Vec<Relocation>,
    /// Function-internal relocations (block branches, returns) - fixed up per function.
    pub(super) function_relocations: Vec<Relocation>,
    /// Whether the current function being lowered is an entry function.
    pub(super) is_entry_function: bool,
}

impl Lowerer {
    /// Create a new lowerer.
    pub fn new() -> Self {
        Self {
            module: None,
            function_addresses: BTreeMap::new(),
            relocations: Vec::new(),
            function_relocations: Vec::new(),
            is_entry_function: false,
        }
    }

    /// Set whether the current function being lowered is an entry function.
    pub fn set_is_entry_function(&mut self, is_entry: bool) {
        self.is_entry_function = is_entry;
    }

    /// Set the module context for function calls.
    pub fn set_module(&mut self, module: lpc_lpir::Module) {
        self.module = Some(module);
    }

    /// Set a function address for call relocations.
    pub fn set_function_address(&mut self, name: String, address: u32) {
        self.function_addresses.insert(name, address);
    }

    /// Get relocations that need to be fixed up.
    pub fn relocations(&self) -> &[Relocation] {
        &self.relocations
    }

    /// Clear relocations (for next function).
    pub fn clear_relocations(&mut self) {
        self.relocations.clear();
        self.function_relocations.clear();
        self.is_entry_function = false;
    }

    /// Get function-internal relocations (for fixup after epilogue).
    pub fn function_relocations(&self) -> &[Relocation] {
        &self.function_relocations
    }

    /// Clear function-internal relocations (called after fixup).
    pub fn clear_function_relocations(&mut self) {
        self.function_relocations.clear();
    }

    /// Lower a function to RISC-V 32-bit code.
    ///
    /// Uses pre-computed allocation, spill/reload plan, and frame layout.
    pub fn lower_function(
        &mut self,
        func: &lpc_lpir::Function,
        allocation: &RegisterAllocation,
        spill_reload: &super::spill_reload::SpillReloadPlan,
        frame_layout: &FrameLayout,
        abi_info: &AbiInfo,
    ) -> Result<InstBuffer, LoweringError> {
        function::lower_function_impl(self, func, allocation, spill_reload, frame_layout, abi_info)
    }

    /// Lower a single instruction.
    pub(super) fn lower_inst(
        &mut self,
        code: &mut InstBuffer,
        inst: &Inst,
        allocation: &RegisterAllocation,
        frame_layout: &FrameLayout,
        abi_info: &AbiInfo,
    ) -> Result<(), LoweringError> {
        match inst {
            Inst::Iconst { result, value } => {
                self.lower_iconst(code, *result, *value, allocation)?;
            }
            Inst::Iadd { result, arg1, arg2 } => {
                self.lower_iadd(code, *result, *arg1, *arg2, allocation, frame_layout)?;
            }
            Inst::Isub { result, arg1, arg2 } => {
                self.lower_isub(code, *result, *arg1, *arg2, allocation, frame_layout)?;
            }
            Inst::Imul { result, arg1, arg2 } => {
                self.lower_imul(code, *result, *arg1, *arg2, allocation, frame_layout)?;
            }
            Inst::IcmpEq { result, arg1, arg2 } => {
                self.lower_icmp_eq(code, *result, *arg1, *arg2, allocation, frame_layout)?;
            }
            Inst::IcmpNe { result, arg1, arg2 } => {
                self.lower_icmp_ne(code, *result, *arg1, *arg2, allocation, frame_layout)?;
            }
            Inst::IcmpLt { result, arg1, arg2 } => {
                self.lower_icmp_lt(code, *result, *arg1, *arg2, allocation, frame_layout)?;
            }
            Inst::IcmpLe { result, arg1, arg2 } => {
                self.lower_icmp_le(code, *result, *arg1, *arg2, allocation, frame_layout)?;
            }
            Inst::IcmpGt { result, arg1, arg2 } => {
                self.lower_icmp_gt(code, *result, *arg1, *arg2, allocation, frame_layout)?;
            }
            Inst::IcmpGe { result, arg1, arg2 } => {
                self.lower_icmp_ge(code, *result, *arg1, *arg2, allocation, frame_layout)?;
            }
            Inst::Return { values } => {
                self.lower_return(code, values, allocation, frame_layout, abi_info)?;
            }
            Inst::Call {
                callee,
                args,
                results,
            } => {
                self.lower_call(
                    code,
                    callee,
                    args,
                    results,
                    allocation,
                    frame_layout,
                    abi_info,
                )?;
            }
            Inst::Jump { .. } => {
                // Jump is handled differently - it can use block_addresses since
                // it's unconditional and we can calculate the offset directly
                // For now, we'll need to pass block_addresses for jumps
                // TODO: Consider making jump use relocations too for consistency
                return Err(LoweringError::UnimplementedInstruction { inst: inst.clone() });
            }
            Inst::Br {
                condition,
                target_true,
                target_false,
            } => {
                self.lower_br(
                    code,
                    *condition,
                    *target_true,
                    *target_false,
                    allocation,
                    frame_layout,
                )?;
            }
            Inst::Halt => {
                code.emit(RiscvInst::Ebreak);
            }
            Inst::Syscall { number, args } => {
                self.lower_syscall(code, *number, args, allocation, frame_layout)?;
            }
            // Known limitation: Some instructions are not yet implemented
            // (Idiv, Irem, Load, Store, etc.). These will return an error.
            _ => {
                return Err(LoweringError::UnimplementedInstruction { inst: inst.clone() });
            }
        }
        Ok(())
    }
}

impl Default for Lowerer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use lpc_lpir::parse_function;

    use super::{
        super::{
            abi::Abi, frame::FrameLayout, liveness::compute_liveness, regalloc::allocate_registers,
            spill_reload::create_spill_reload_plan,
        },
        *,
    };

    #[test]
    fn test_lower_error_missing_value() {
        // Test that missing values in allocation return an error
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
        let mut allocation = allocate_registers(&func, &liveness);

        // Remove a value from allocation to simulate error
        let v0 = lpc_lpir::Value::new(0);
        allocation.value_to_reg.remove(&v0);
        allocation.value_to_slot.remove(&v0);

        let spill_reload = create_spill_reload_plan(&func, &allocation, &liveness);

        let has_calls = false;
        let frame_layout = FrameLayout::compute(
            &allocation.used_callee_saved,
            allocation.spill_slot_count,
            has_calls,
            func.signature.params.len(),
            0,
            func.signature.returns.len(),
            0,
        );

        let abi_info = Abi::compute_abi_info(&func, &allocation, 0);

        let mut lowerer = Lowerer::new();
        let result =
            lowerer.lower_function(&func, &allocation, &spill_reload, &frame_layout, &abi_info);

        // Should return an error
        assert!(result.is_err());
        match result {
            Err(LoweringError::ValueNotAllocated { value }) => {
                assert_eq!(value, v0);
            }
            Err(e) => panic!("Expected ValueNotAllocated error, got {:?}", e),
            Ok(_) => panic!("Expected error but got Ok"),
        }
    }

    #[test]
    fn test_lower_error_result_not_in_register() {
        // Test that result values must be in registers
        let ir = r#"
function %test() -> i32 {
block0:
    v0 = iconst 1
    return v0
}"#;

        let func = parse_function(ir).expect("Failed to parse IR function");
        let liveness = compute_liveness(&func);
        let mut allocation = allocate_registers(&func, &liveness);

        // Remove result value from register allocation (but keep it spilled)
        let v0 = lpc_lpir::Value::new(0);
        allocation.value_to_reg.remove(&v0);
        // Add it to value_to_slot to simulate a spilled result value
        if !allocation.value_to_slot.contains_key(&v0) {
            allocation.value_to_slot.insert(v0, 0);
        }

        let spill_reload = create_spill_reload_plan(&func, &allocation, &liveness);

        let has_calls = false;
        // Include temporary spill slots needed for caller-saved register preservation
        let total_spill_slots = allocation.spill_slot_count + spill_reload.max_temp_spill_slots;
        let frame_layout = FrameLayout::compute(
            &allocation.used_callee_saved,
            total_spill_slots,
            has_calls,
            func.signature.params.len(),
            0,
            func.signature.returns.len(),
            0,
        );

        let abi_info = Abi::compute_abi_info(&func, &allocation, 0);

        let mut lowerer = Lowerer::new();
        let result =
            lowerer.lower_function(&func, &allocation, &spill_reload, &frame_layout, &abi_info);

        // Should return an error for result not in register
        assert!(result.is_err());
        match result {
            Err(LoweringError::ResultNotInRegister { value }) => {
                assert_eq!(value, v0);
            }
            Err(e) => panic!("Expected ResultNotInRegister error, got {:?}", e),
            Ok(_) => panic!("Expected error but got Ok"),
        }
    }

    #[test]
    fn test_lower_error_unimplemented_instruction() {
        // Test that unimplemented instructions return an error
        let ir = r#"
function %test() -> i32 {
block0:
    v0 = iconst 1
    v1 = idiv v0, v0
    return v1
}"#;

        let func = parse_function(ir).expect("Failed to parse IR function");
        let liveness = compute_liveness(&func);
        let allocation = allocate_registers(&func, &liveness);
        let spill_reload = create_spill_reload_plan(&func, &allocation, &liveness);

        let has_calls = false;
        let total_spill_slots = allocation.spill_slot_count + spill_reload.max_temp_spill_slots;
        let frame_layout = FrameLayout::compute(
            &allocation.used_callee_saved,
            total_spill_slots,
            has_calls,
            func.signature.params.len(),
            0,
            func.signature.returns.len(),
            0,
        );

        let abi_info = Abi::compute_abi_info(&func, &allocation, 0);

        let mut lowerer = Lowerer::new();
        let result =
            lowerer.lower_function(&func, &allocation, &spill_reload, &frame_layout, &abi_info);

        // Should return an error for unimplemented instruction
        assert!(result.is_err());
        match result {
            Err(LoweringError::UnimplementedInstruction { .. }) => {
                // Expected
            }
            Err(e) => panic!("Expected UnimplementedInstruction error, got {:?}", e),
            Ok(_) => panic!("Expected error but got Ok"),
        }
    }
}
