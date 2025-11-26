//! Frame layout computation for RISC-V 32-bit functions.
//!
//! This module implements frame layout computation aligned with Cranelift's
//! architecture. The frame layout is pre-computed before code generation.

use alloc::vec::Vec;

use crate::Gpr;
use super::lower::ByteOffset;

/// Storage location for a value in the frame.
///
/// This enum represents where a value is stored, centralizing the decision
/// logic that was previously scattered across multiple files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageLocation {
    /// Value is in a register.
    Register(Gpr),
    /// Value is in a spill slot.
    SpillSlot { slot: u32, offset: ByteOffset },
    /// Value is an incoming stack argument (index >= 8).
    IncomingStackArg { index: usize, offset: ByteOffset },
    /// Value is an outgoing stack argument (index >= 8).
    OutgoingStackArg { index: usize, offset: ByteOffset },
    /// Value is a callee-saved register that was saved to the frame.
    CalleeSaved { reg: Gpr, offset: ByteOffset },
}

/// Frame layout for a RISC-V 32-bit function.
///
/// The frame layout follows Cranelift's model:
/// ```text
/// ┌─────────────────────────────────────┐
/// │  Caller's Stack Frame               │
/// ├─────────────────────────────────────┤
/// │  Outgoing Arguments (if any)        │  ← SP (after call)
/// ├─────────────────────────────────────┤
/// │  Spill Slots                        │
/// ├─────────────────────────────────────┤
/// │  Clobber Area (callee-saved regs)   │
/// ├─────────────────────────────────────┤
/// │  Setup Area (FP/LR)                 │  ← FP (if used)
/// ├─────────────────────────────────────┤
/// │  Incoming Arguments (if any)        │
/// └─────────────────────────────────────┘
/// ```
#[derive(Debug, Clone)]
pub struct FrameLayout {
    /// Word size in bytes (4 for RISC-V 32-bit)
    pub word_bytes: u32,

    /// Size of incoming arguments on stack (if > 8 args)
    pub incoming_args_size: u32,

    /// Size of setup area (FP/LR save area, 0 or 8 bytes)
    pub setup_area_size: u32,

    /// Size of clobber area (callee-saved registers)
    pub clobber_size: u32,

    /// Size of fixed frame storage (spill slots)
    pub fixed_frame_storage_size: u32,

    /// Size of outgoing arguments area
    pub outgoing_args_size: u32,

    /// List of callee-saved registers that need saving
    pub clobbered_callee_saves: Vec<Gpr>,

    /// Whether function makes calls
    pub has_function_calls: bool,
}

/// Align a size to 16 bytes (RISC-V ABI requirement).
fn align_to_16(size: u32) -> u32 {
    (size + 15) & !15
}

impl FrameLayout {
    /// Compute frame layout for a function.
    ///
    /// # Arguments
    ///
    /// * `used_callee_saved` - List of callee-saved registers that are used
    /// * `spill_slots` - Number of spill slots needed
    /// * `has_calls` - Whether the function makes function calls
    /// * `incoming_args` - Number of incoming arguments (for stack args)
    /// * `outgoing_args` - Number of outgoing arguments (for stack args)
    pub fn compute(
        used_callee_saved: &[Gpr],
        spill_slots: usize,
        has_calls: bool,
        incoming_args: usize,
        outgoing_args: usize,
    ) -> Self {
        #[cfg(feature = "debug-lowering")]
        crate::debug_lowering!(
            "FrameLayout::compute: used_callee_saved={}, spill_slots={}, has_calls={}, incoming_args={}, outgoing_args={}",
            used_callee_saved.len(),
            spill_slots,
            has_calls,
            incoming_args,
            outgoing_args
        );
        // Determine if setup area needed
        // Setup area is needed if we have calls, use callee-saved regs, or need spill slots
        let setup_area_size = if has_calls || !used_callee_saved.is_empty() || spill_slots > 0 {
            8 // FP/LR save area (8 bytes for RISC-V 32-bit: 4 for RA, 4 for FP if used)
        } else {
            0
        };

        // Compute clobber size (callee-saved registers)
        // Each register is 4 bytes
        let clobber_size = align_to_16(used_callee_saved.len() as u32 * 4);

        // Compute fixed frame storage (spill slots)
        // Each spill slot is 4 bytes
        let fixed_frame_storage_size = align_to_16(spill_slots as u32 * 4);

        // Compute incoming args size (if > 8 args, they go on stack)
        let incoming_args_size = if incoming_args > 8 {
            align_to_16((incoming_args - 8) as u32 * 4)
        } else {
            0
        };

        // Compute outgoing args size (if > 8 args, they go on stack)
        let outgoing_args_size = if outgoing_args > 8 {
            align_to_16((outgoing_args - 8) as u32 * 4)
        } else {
            0
        };

        let layout = FrameLayout {
            word_bytes: 4,
            incoming_args_size,
            setup_area_size,
            clobber_size,
            fixed_frame_storage_size,
            outgoing_args_size,
            clobbered_callee_saves: used_callee_saved.to_vec(),
            has_function_calls: has_calls,
        };

        #[cfg(feature = "debug-lowering")]
        crate::debug_lowering!(
            "FrameLayout::compute result: setup_area={}, clobber={}, spills={}, total={}",
            layout.setup_area_size,
            layout.clobber_size,
            layout.fixed_frame_storage_size,
            layout.total_size()
        );

        layout
    }

    /// Get the total frame size in bytes.
    pub fn total_size(&self) -> u32 {
        let size = self.setup_area_size
            + self.clobber_size
            + self.fixed_frame_storage_size
            + self.outgoing_args_size;
        crate::debug!("[FRAME] total_size(): setup={}, clobber={}, spills={}, outgoing_args={}, total={}", 
            self.setup_area_size, self.clobber_size, self.fixed_frame_storage_size, 
            self.outgoing_args_size, size);
        size
    }

    /// Get the stack offset for a callee-saved register.
    ///
    /// Returns the offset from SP where the register is saved.
    pub fn callee_saved_offset(&self, reg: Gpr) -> Option<ByteOffset> {
        self.clobbered_callee_saves
            .iter()
            .position(|&r| r.num() == reg.num())
            .map(|idx| {
                let offset = self.setup_area_size + (idx as u32 * 4);
                ByteOffset(-(offset as i32))
            })
    }

    /// Get the stack offset for a spill slot.
    ///
    /// Returns the offset from SP where the spill slot is located.
    pub fn spill_slot_offset(&self, slot: u32) -> ByteOffset {
        let base_offset = self.setup_area_size + self.clobber_size;
        let offset = ByteOffset(-((base_offset + slot * 4) as i32));
        #[cfg(feature = "debug-lowering")]
        crate::debug_lowering!(
            "spill_slot_offset(slot={}): base={}, offset={}",
            slot,
            base_offset,
            offset.as_i32()
        );
        offset
    }

    /// Get stack offset for incoming argument (index >= 8)
    ///
    /// Returns offset relative to SP **before** prologue (positive offset).
    /// After prologue, the actual offset is: total_size() + offset
    pub fn incoming_arg_offset(&self, arg_index: usize) -> Option<ByteOffset> {
        if arg_index < 8 {
            return None; // In register
        }
        let stack_index = arg_index - 8;
        Some(ByteOffset((stack_index * 4) as i32))
    }

    /// Get stack offset for outgoing argument (index >= 8)
    ///
    /// Returns offset relative to SP (positive offset, per RISC-V convention).
    /// Stack arguments are stored at positive offsets from SP.
    /// The caller stores them, and the callee reads them at the same positive offsets.
    pub fn outgoing_arg_offset(&self, arg_index: usize) -> Option<ByteOffset> {
        if arg_index < 8 {
            return None; // In register
        }
        let stack_index = arg_index - 8;
        // Stack arguments start at SP + 0, each is 4 bytes
        let offset = ByteOffset((stack_index * 4) as i32);
        crate::debug!("[FRAME] outgoing_arg_offset(arg_index={}): stack_index={}, offset={}", arg_index, stack_index, offset.as_i32());
        Some(offset)
    }

    /// Get stack offset for return value (index >= 8)
    ///
    /// Similar to incoming args, but caller allocates space
    pub fn return_value_offset(&self, ret_index: usize) -> Option<ByteOffset> {
        if ret_index < 8 {
            return None; // In register
        }
        let stack_index = ret_index - 8;
        Some(ByteOffset((stack_index * 4) as i32))
    }

    /// Get the storage location for storing a value.
    ///
    /// This centralizes the decision logic for where to store a value,
    /// checking register allocation and returning the appropriate storage location.
    pub fn store_value_location(
        &self,
        value: lpc_lpir::Value,
        allocation: &crate::backend::RegisterAllocation,
    ) -> Option<StorageLocation> {
        // Check if value is in a register
        if let Some(reg) = allocation.value_to_reg.get(&value) {
            return Some(StorageLocation::Register(*reg));
        }

        // Check if value is in a spill slot
        if let Some(slot) = allocation.value_to_slot.get(&value) {
            let offset = self.spill_slot_offset(*slot);
            return Some(StorageLocation::SpillSlot {
                slot: *slot,
                offset,
            });
        }

        None
    }

    /// Get the storage location for loading a value.
    ///
    /// Similar to `store_value_location`, but used when loading a value.
    /// This centralizes the decision logic for where to load a value from.
    pub fn load_value_location(
        &self,
        value: lpc_lpir::Value,
        allocation: &crate::backend::RegisterAllocation,
    ) -> Option<StorageLocation> {
        self.store_value_location(value, allocation)
    }

    /// Get the incoming argument offset after prologue.
    ///
    /// This computes the actual offset accounting for SP adjustment after prologue.
    /// Incoming stack arguments are at positive offsets from SP before prologue,
    /// but after prologue, SP is adjusted, so the offset becomes: total_size() + original_offset.
    pub fn incoming_arg_offset_after_prologue(&self, arg_index: usize) -> Option<ByteOffset> {
        if arg_index < 8 {
            return None; // In register
        }
        let stack_index = arg_index - 8;
        // After prologue, SP is adjusted by total_size(), so we add that to the original offset
        let original_offset = (stack_index * 4) as i32;
        Some(ByteOffset(original_offset + self.total_size() as i32))
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    #[test]
    fn test_compute_frame_layout_no_calls() {
        let layout = FrameLayout::compute(&[], 0, false, 0, 0);
        assert_eq!(layout.setup_area_size, 0);
        assert_eq!(layout.clobber_size, 0);
        assert_eq!(layout.fixed_frame_storage_size, 0);
        assert_eq!(layout.outgoing_args_size, 0);
        assert_eq!(layout.total_size(), 0);
    }

    #[test]
    fn test_compute_frame_layout_with_calls() {
        let layout = FrameLayout::compute(&[], 0, true, 0, 0);
        assert_eq!(layout.setup_area_size, 8);
        assert_eq!(layout.has_function_calls, true);
        assert_eq!(layout.total_size(), 8);
    }

    #[test]
    fn test_compute_frame_layout_with_spills() {
        let layout = FrameLayout::compute(&[], 2, false, 0, 0);
        assert_eq!(layout.setup_area_size, 8);
        assert_eq!(layout.fixed_frame_storage_size, 16); // Aligned to 16
        assert_eq!(layout.total_size(), 24);
    }

    #[test]
    fn test_compute_frame_layout_with_callee_saved() {
        let used = vec![Gpr::S0, Gpr::S1];
        let layout = FrameLayout::compute(&used, 0, false, 0, 0);
        assert_eq!(layout.setup_area_size, 8);
        assert_eq!(layout.clobber_size, 16); // Aligned to 16
        assert_eq!(layout.clobbered_callee_saves.len(), 2);
        assert_eq!(layout.total_size(), 24);
    }

    #[test]
    fn test_callee_saved_offset() {
        let used = vec![Gpr::S0, Gpr::S1];
        let layout = FrameLayout::compute(&used, 0, false, 0, 0);

        // S0 should be at offset -8 (after setup area)
        let offset_s0 = layout.callee_saved_offset(Gpr::S0).unwrap();
        assert_eq!(offset_s0.as_i32(), -8);

        // S1 should be at offset -12 (after S0)
        let offset_s1 = layout.callee_saved_offset(Gpr::S1).unwrap();
        assert_eq!(offset_s1.as_i32(), -12);
    }

    #[test]
    fn test_spill_slot_offset() {
        let layout = FrameLayout::compute(&[], 2, false, 0, 0);

        // First spill slot should be after setup area (8 bytes)
        let offset_slot0 = layout.spill_slot_offset(0);
        assert_eq!(offset_slot0.as_i32(), -8);

        // Second spill slot should be 4 bytes after first
        let offset_slot1 = layout.spill_slot_offset(1);
        assert_eq!(offset_slot1.as_i32(), -12);
    }

    #[test]
    fn test_outgoing_args_size() {
        // Test with 10 outgoing args (2 need to go on stack)
        let layout = FrameLayout::compute(&[], 0, true, 0, 10);
        assert_eq!(layout.outgoing_args_size, 16); // (10-8)*4 = 8, aligned to 16
    }

    #[test]
    fn test_incoming_args_size() {
        // Test with 10 incoming args (2 need to go on stack)
        let layout = FrameLayout::compute(&[], 0, true, 10, 0);
        assert_eq!(layout.incoming_args_size, 16); // (10-8)*4 = 8, aligned to 16
    }

    #[test]
    fn test_incoming_arg_offset() {
        let layout = FrameLayout::compute(&[], 0, true, 10, 0);

        // First 8 args should return None (in registers)
        for i in 0..8 {
            assert_eq!(layout.incoming_arg_offset(i), None);
        }

        // Stack args should have positive offsets (relative to SP before prologue)
        assert_eq!(layout.incoming_arg_offset(8), Some(ByteOffset(0)));
        assert_eq!(layout.incoming_arg_offset(9), Some(ByteOffset(4)));
        assert_eq!(layout.incoming_arg_offset(10), Some(ByteOffset(8)));
    }

    #[test]
    fn test_outgoing_arg_offset() {
        let layout = FrameLayout::compute(&[], 0, true, 0, 10);

        // First 8 args should return None (in registers)
        for i in 0..8 {
            assert_eq!(layout.outgoing_arg_offset(i), None);
        }

        // Stack args should have positive offsets (per RISC-V convention)
        // Stack arguments start at SP + 0, each is 4 bytes
        assert_eq!(layout.outgoing_arg_offset(8), Some(ByteOffset(0)));
        assert_eq!(layout.outgoing_arg_offset(9), Some(ByteOffset(4)));
        assert_eq!(layout.outgoing_arg_offset(10), Some(ByteOffset(8)));
    }

    #[test]
    fn test_return_value_offset() {
        let layout = FrameLayout::compute(&[], 0, true, 0, 0);

        // First 8 return values should return None (in registers)
        for i in 0..8 {
            assert_eq!(layout.return_value_offset(i), None);
        }

        // Stack returns should have positive offsets (relative to SP before prologue)
        assert_eq!(layout.return_value_offset(8), Some(ByteOffset(0)));
        assert_eq!(layout.return_value_offset(9), Some(ByteOffset(4)));
        assert_eq!(layout.return_value_offset(10), Some(ByteOffset(8)));
    }
}
