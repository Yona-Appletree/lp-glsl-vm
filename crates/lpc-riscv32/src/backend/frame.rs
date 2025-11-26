//! Frame layout computation for RISC-V 32-bit functions.
//!
//! This module implements frame layout computation aligned with Cranelift's
//! architecture. The frame layout is pre-computed before code generation.

use alloc::vec::Vec;

use super::lower::ByteOffset;
use crate::Gpr;

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
/// │  Outgoing Arguments (if any)        │  ← SP (after prologue, bottom of frame)
/// ├─────────────────────────────────────┤
/// │  Setup Area (FP/LR)                 │
/// ├─────────────────────────────────────┤
/// │  Clobber Area (callee-saved regs)   │
/// ├─────────────────────────────────────┤
/// │  Spill Slots                        │
/// └─────────────────────────────────────┘
/// ```
///
/// Key points:
/// - Outgoing args are stored at SP+0, SP+4, etc. (relative to SP after prologue)
/// - Incoming args are loaded from SP+0, SP+4, etc. (relative to SP before prologue)
/// - Since caller's SP (after prologue) = callee's SP (before prologue), offsets match
/// - The frame grows downward: SP is adjusted by total_size() in prologue
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

    /// Size of tail-args area (largest of incoming args, outgoing args + stack returns)
    /// This area is at the bottom of the frame and persists after epilogue
    pub tail_args_size: u32,

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
    /// * `return_count` - Number of return values (for computing stack return area)
    /// * `max_callee_stack_returns` - Maximum stack return area needed by any callee
    pub fn compute(
        used_callee_saved: &[Gpr],
        spill_slots: usize,
        has_calls: bool,
        incoming_args: usize,
        outgoing_args: usize,
        return_count: usize,
        max_callee_stack_returns: usize,
    ) -> Self {
        #[cfg(feature = "debug-lowering")]
        crate::debug_lowering!(
            "FrameLayout::compute: used_callee_saved={}, spill_slots={}, has_calls={}, \
             incoming_args={}, outgoing_args={}",
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

        // Compute stack return area size (if > 8 returns, they go on stack)
        let stack_return_area = if return_count > 8 {
            align_to_16((return_count - 8) as u32 * 4)
        } else {
            0
        };

        // Compute max callee stack return area
        let max_callee_stack_return_area = if max_callee_stack_returns > 8 {
            align_to_16((max_callee_stack_returns - 8) as u32 * 4)
        } else {
            0
        };

        // Tail-args size is the maximum of:
        // 1. Incoming stack args for this function
        // 2. Outgoing stack args + stack return area needed by callees
        // 3. Stack return area for this function's returns
        let tail_args_size = incoming_args_size
            .max(outgoing_args_size + max_callee_stack_return_area)
            .max(stack_return_area);

        let layout = FrameLayout {
            word_bytes: 4,
            incoming_args_size,
            setup_area_size,
            clobber_size,
            fixed_frame_storage_size,
            outgoing_args_size,
            tail_args_size,
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
    ///
    /// This includes tail_args_size, setup_area_size, clobber_size, and
    /// fixed_frame_storage_size. The frame is laid out as:
    /// [tail-args] [setup] [clobber] [spills]
    /// SP points to the bottom (tail-args area) after prologue.
    pub fn total_size(&self) -> u32 {
        let size = self.tail_args_size
            + self.setup_area_size
            + self.clobber_size
            + self.fixed_frame_storage_size;
        crate::debug!(
            "[FRAME] total_size(): tail_args={}, setup={}, clobber={}, spills={}, total={}",
            self.tail_args_size,
            self.setup_area_size,
            self.clobber_size,
            self.fixed_frame_storage_size,
            size
        );
        size
    }

    /// Get the offset of the outgoing args base (relative to adjusted SP).
    ///
    /// Outgoing args are at the bottom of the frame: SP+0 to SP+(outgoing_args_size-1)
    pub fn outgoing_args_base_offset(&self) -> ByteOffset {
        ByteOffset(0)
    }

    /// Get the offset of the setup area base (relative to adjusted SP).
    ///
    /// Setup area is above tail-args: SP+tail_args_size to SP+tail_args_size+setup_area_size-1
    pub fn setup_area_base_offset(&self) -> ByteOffset {
        ByteOffset(self.tail_args_size as i32)
    }

    /// Get the offset of the clobber area base (relative to adjusted SP).
    ///
    /// Clobber area is above the setup area.
    pub fn clobber_area_base_offset(&self) -> ByteOffset {
        ByteOffset((self.tail_args_size + self.setup_area_size) as i32)
    }

    /// Get the offset of spill slots base (relative to adjusted SP).
    ///
    /// Spill slots are above the clobber area.
    pub fn spill_slots_base_offset(&self) -> ByteOffset {
        ByteOffset((self.tail_args_size + self.setup_area_size + self.clobber_size) as i32)
    }

    /// Get the stack offset for a callee-saved register.
    ///
    /// Returns the offset from SP where the register is saved.
    /// Negative offset because frame grows downward from SP.
    pub fn callee_saved_offset(&self, reg: Gpr) -> Option<ByteOffset> {
        self.clobbered_callee_saves
            .iter()
            .position(|&r| r.num() == reg.num())
            .map(|idx| {
                let base = self.tail_args_size + self.setup_area_size;
                let offset = base + (idx as u32 * 4);
                ByteOffset(-(offset as i32))
            })
    }

    /// Get the stack offset for a spill slot.
    ///
    /// Returns the offset from SP where the spill slot is located.
    /// Negative offset because frame grows downward from SP.
    pub fn spill_slot_offset(&self, slot: u32) -> ByteOffset {
        let base_offset = self.tail_args_size + self.setup_area_size + self.clobber_size;
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
    /// Returns offset relative to callee's SP (before prologue).
    /// This equals the caller's SP (after prologue).
    ///
    /// The offset is (idx-8)*4, which matches outgoing_arg_offset because:
    /// - Caller stores outgoing args at SP + (idx-8)*4 (after prologue)
    /// - Callee loads incoming args from SP + (idx-8)*4 (before prologue)
    /// - Since caller's SP (after prologue) = callee's SP (before prologue), offsets match
    pub fn incoming_arg_offset(&self, arg_index: usize) -> Option<ByteOffset> {
        if arg_index < 8 {
            return None; // In register
        }
        let stack_index = arg_index - 8;
        Some(ByteOffset((stack_index * 4) as i32))
    }

    /// Get stack offset for outgoing argument (index >= 8)
    ///
    /// Returns offset relative to caller's adjusted SP (positive offset).
    /// Stack arguments are stored at SP + (idx-8)*4, which matches incoming_arg_offset
    /// because caller's SP (after prologue) = callee's SP (before prologue).
    pub fn outgoing_arg_offset(&self, arg_index: usize) -> Option<ByteOffset> {
        if arg_index < 8 {
            return None; // In register
        }
        let stack_index = arg_index - 8;
        // Outgoing args are stored at the bottom of the frame (SP+0, SP+4, etc.)
        let offset = ByteOffset((stack_index * 4) as i32);
        crate::debug!(
            "[FRAME] outgoing_arg_offset(arg_index={}): stack_index={}, offset={}",
            arg_index,
            stack_index,
            offset.as_i32()
        );
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
    /// Incoming stack arguments are loaded BEFORE prologue (at SP + (idx-8)*4).
    /// After prologue, SP is adjusted downward by total_size(), so the offset becomes
    /// negative: -(total_size() - (idx-8)*4) = (idx-8)*4 - total_size()
    ///
    /// However, this method is typically not needed because incoming args should
    /// be loaded before the prologue adjusts SP.
    pub fn incoming_arg_offset_after_prologue(&self, arg_index: usize) -> Option<ByteOffset> {
        if arg_index < 8 {
            return None; // In register
        }
        let stack_index = arg_index - 8;
        // After prologue, SP is adjusted downward, so incoming args are at negative offsets
        let original_offset = (stack_index * 4) as i32;
        Some(ByteOffset(original_offset - self.total_size() as i32))
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    #[test]
    fn test_compute_frame_layout_no_calls() {
        let layout = FrameLayout::compute(&[], 0, false, 0, 0, 0, 0);
        assert_eq!(layout.setup_area_size, 0);
        assert_eq!(layout.clobber_size, 0);
        assert_eq!(layout.fixed_frame_storage_size, 0);
        assert_eq!(layout.outgoing_args_size, 0);
        assert_eq!(layout.total_size(), 0);
    }

    #[test]
    fn test_compute_frame_layout_with_calls() {
        let layout = FrameLayout::compute(&[], 0, true, 0, 0, 0, 0);
        assert_eq!(layout.setup_area_size, 8);
        assert_eq!(layout.has_function_calls, true);
        assert_eq!(layout.outgoing_args_size, 0);
        assert_eq!(layout.total_size(), 8); // setup only
    }

    #[test]
    fn test_compute_frame_layout_with_spills() {
        let layout = FrameLayout::compute(&[], 2, false, 0, 0, 0, 0);
        assert_eq!(layout.setup_area_size, 8);
        assert_eq!(layout.fixed_frame_storage_size, 16); // Aligned to 16
        assert_eq!(layout.outgoing_args_size, 0);
        assert_eq!(layout.total_size(), 24); // setup + spills
    }

    #[test]
    fn test_compute_frame_layout_with_callee_saved() {
        let used = vec![Gpr::S0, Gpr::S1];
        let layout = FrameLayout::compute(&used, 0, false, 0, 0, 0, 0);
        assert_eq!(layout.setup_area_size, 8);
        assert_eq!(layout.clobber_size, 16); // Aligned to 16
        assert_eq!(layout.clobbered_callee_saves.len(), 2);
        assert_eq!(layout.total_size(), 24);
    }

    #[test]
    fn test_callee_saved_offset() {
        let used = vec![Gpr::S0, Gpr::S1];
        let layout = FrameLayout::compute(&used, 0, false, 0, 0, 0, 0);

        // S0 should be at offset -(outgoing_args + setup_area) = -(0 + 8) = -8
        let offset_s0 = layout.callee_saved_offset(Gpr::S0).unwrap();
        assert_eq!(offset_s0.as_i32(), -8);

        // S1 should be at offset -12 (after S0)
        let offset_s1 = layout.callee_saved_offset(Gpr::S1).unwrap();
        assert_eq!(offset_s1.as_i32(), -12);
    }

    #[test]
    fn test_spill_slot_offset() {
        let layout = FrameLayout::compute(&[], 2, false, 0, 0, 0, 0);

        // First spill slot should be after setup area (8 bytes)
        // Offset = -(tail_args + setup_area) = -(0 + 8) = -8
        let offset_slot0 = layout.spill_slot_offset(0);
        assert_eq!(offset_slot0.as_i32(), -8);

        // Second spill slot should be 4 bytes after first
        let offset_slot1 = layout.spill_slot_offset(1);
        assert_eq!(offset_slot1.as_i32(), -12);
    }

    #[test]
    fn test_outgoing_args_size() {
        // Test with 10 outgoing args (2 need to go on stack)
        let layout = FrameLayout::compute(&[], 0, true, 0, 10, 0, 0);
        assert_eq!(layout.outgoing_args_size, 16); // (10-8)*4 = 8, aligned to 16
    }

    #[test]
    fn test_incoming_args_size() {
        // Test with 10 incoming args (2 need to go on stack)
        let layout = FrameLayout::compute(&[], 0, true, 10, 0, 0, 0);
        assert_eq!(layout.incoming_args_size, 16); // (10-8)*4 = 8, aligned to 16
    }

    #[test]
    fn test_incoming_arg_offset() {
        let layout = FrameLayout::compute(&[], 0, true, 10, 0, 0, 0);

        // First 8 args should return None (in registers)
        for i in 0..8 {
            assert_eq!(layout.incoming_arg_offset(i), None);
        }

        // Stack args should have offsets (idx-8)*4 (relative to SP before prologue)
        // These match outgoing_arg_offset because caller's SP (after prologue) = callee's SP (before prologue)
        assert_eq!(layout.incoming_arg_offset(8), Some(ByteOffset(0))); // SP + 0
        assert_eq!(layout.incoming_arg_offset(9), Some(ByteOffset(4))); // SP + 4
        assert_eq!(layout.incoming_arg_offset(10), Some(ByteOffset(8))); // SP + 8
    }

    #[test]
    fn test_outgoing_arg_offset() {
        // Test with a frame that has setup area (8 bytes)
        let layout = FrameLayout::compute(&[], 0, true, 0, 10, 0, 0);

        // First 8 args should return None (in registers)
        for i in 0..8 {
            assert_eq!(layout.outgoing_arg_offset(i), None);
        }

        // Stack args should be stored at the bottom of the frame (SP+0, SP+4, etc.)
        // These match incoming_arg_offset because caller's SP (after prologue) = callee's SP (before prologue)
        assert_eq!(layout.outgoing_arg_offset(8), Some(ByteOffset(0))); // SP + 0
        assert_eq!(layout.outgoing_arg_offset(9), Some(ByteOffset(4))); // SP + 4
        assert_eq!(layout.outgoing_arg_offset(10), Some(ByteOffset(8))); // SP + 8
    }

    #[test]
    fn test_outgoing_arg_offset_with_spills() {
        // Test with spills - outgoing args are still at the bottom of the frame
        let layout = FrameLayout::compute(&[], 2, true, 0, 10, 0, 0);

        // Outgoing args are at SP+0, SP+4, etc. (not affected by spills)
        assert_eq!(layout.outgoing_arg_offset(8), Some(ByteOffset(0))); // SP + 0
        assert_eq!(layout.outgoing_arg_offset(9), Some(ByteOffset(4))); // SP + 4
                                                                        // total_size includes tail_args(16) + setup(8) + spills(16) = 40
        assert_eq!(layout.total_size(), 40);
    }

    #[test]
    fn test_incoming_and_outgoing_same_offset() {
        // Incoming and outgoing args should use the same offsets
        // because caller's SP (after prologue) = callee's SP (before prologue)
        let layout = FrameLayout::compute(&[], 0, true, 10, 10, 0, 0);

        for i in 8..=10 {
            let incoming = layout.incoming_arg_offset(i);
            let outgoing = layout.outgoing_arg_offset(i);
            assert_eq!(
                incoming, outgoing,
                "Incoming and outgoing offsets should match for arg {}",
                i
            );
        }
    }

    #[test]
    fn test_frame_offset_helpers() {
        let layout = FrameLayout::compute(&[Gpr::S0], 2, true, 0, 0, 0, 0);

        // Outgoing args are at the bottom (SP+0)
        assert_eq!(layout.outgoing_args_base_offset().as_i32(), 0);

        // Setup area is above tail-args
        assert_eq!(
            layout.setup_area_base_offset().as_i32(),
            layout.tail_args_size as i32
        );

        // Clobber area is above setup area
        assert_eq!(
            layout.clobber_area_base_offset().as_i32(),
            (layout.tail_args_size + layout.setup_area_size) as i32
        );

        // Spill slots are above clobber area
        let expected_spill_base =
            layout.tail_args_size + layout.setup_area_size + layout.clobber_size;
        assert_eq!(
            layout.spill_slots_base_offset().as_i32(),
            expected_spill_base as i32
        );
    }

    #[test]
    fn test_return_value_offset() {
        let layout = FrameLayout::compute(&[], 0, true, 0, 0, 0, 0);

        // First 8 return values should return None (in registers)
        for i in 0..8 {
            assert_eq!(layout.return_value_offset(i), None);
        }

        // Stack returns should have positive offsets (relative to SP before prologue)
        assert_eq!(layout.return_value_offset(8), Some(ByteOffset(0)));
        assert_eq!(layout.return_value_offset(9), Some(ByteOffset(4)));
        assert_eq!(layout.return_value_offset(10), Some(ByteOffset(8)));
    }

    #[test]
    fn test_outgoing_args_at_bottom_of_frame() {
        // Verify outgoing args are at the bottom of the frame (offset 0)
        let layout = FrameLayout::compute(&[], 0, true, 0, 10, 0, 0);

        // Outgoing args should start at offset 0
        assert_eq!(layout.outgoing_args_base_offset().as_i32(), 0);
        assert_eq!(layout.outgoing_arg_offset(8), Some(ByteOffset(0)));

        // Setup area should be above tail-args
        assert_eq!(
            layout.setup_area_base_offset().as_i32(),
            layout.tail_args_size as i32
        );
    }

    #[test]
    fn test_total_size_includes_tail_args() {
        // Verify total_size includes tail_args_size
        let layout = FrameLayout::compute(&[], 0, true, 0, 10, 0, 0);

        // total_size = tail_args(16) + setup(8) = 24
        let expected_total = layout.tail_args_size + layout.setup_area_size;
        assert_eq!(layout.total_size(), expected_total);
        assert_eq!(layout.total_size(), 24);
    }

    #[test]
    fn test_frame_layout_order_with_all_components() {
        // Test frame layout order: tail-args -> setup -> clobber -> spills
        let used = vec![Gpr::S0];
        let layout = FrameLayout::compute(&used, 2, true, 0, 10, 0, 0);

        // Verify order: tail-args at bottom (0)
        assert_eq!(layout.outgoing_args_base_offset().as_i32(), 0);

        // Setup area above tail-args
        assert_eq!(
            layout.setup_area_base_offset().as_i32(),
            layout.tail_args_size as i32
        );

        // Clobber area above setup
        assert_eq!(
            layout.clobber_area_base_offset().as_i32(),
            (layout.tail_args_size + layout.setup_area_size) as i32
        );

        // Spills above clobber
        assert_eq!(
            layout.spill_slots_base_offset().as_i32(),
            (layout.tail_args_size + layout.setup_area_size + layout.clobber_size) as i32
        );

        // Total size should include all components
        let expected_total = layout.tail_args_size
            + layout.setup_area_size
            + layout.clobber_size
            + layout.fixed_frame_storage_size;
        assert_eq!(layout.total_size(), expected_total);
    }

    #[test]
    fn test_callee_saved_offset_with_tail_args() {
        // Verify callee-saved offsets account for tail-args
        let used = vec![Gpr::S0];
        let layout = FrameLayout::compute(&used, 0, true, 0, 10, 0, 0);

        // S0 should be at offset -(tail_args + setup_area) = -(16 + 8) = -24
        let offset_s0 = layout.callee_saved_offset(Gpr::S0).unwrap();
        assert_eq!(offset_s0.as_i32(), -24);
    }

    #[test]
    fn test_spill_slot_offset_with_tail_args() {
        // Verify spill slot offsets account for tail-args
        let layout = FrameLayout::compute(&[], 2, true, 0, 10, 0, 0);

        // First spill slot should be at -(tail_args + setup_area) = -(16 + 8) = -24
        let offset_slot0 = layout.spill_slot_offset(0);
        assert_eq!(offset_slot0.as_i32(), -24);

        // Second spill slot should be 4 bytes after first
        let offset_slot1 = layout.spill_slot_offset(1);
        assert_eq!(offset_slot1.as_i32(), -28);
    }

    #[test]
    fn test_incoming_outgoing_offset_match_with_frame() {
        // Verify incoming and outgoing offsets match even with complex frame
        let used = vec![Gpr::S0];
        let layout = FrameLayout::compute(&used, 2, true, 10, 10, 0, 0);

        // Both should use same offsets regardless of frame complexity
        for i in 8..=10 {
            let incoming = layout.incoming_arg_offset(i);
            let outgoing = layout.outgoing_arg_offset(i);
            assert_eq!(
                incoming, outgoing,
                "Incoming and outgoing offsets should match for arg {} even with complex frame",
                i
            );
            // Both should be simple (idx-8)*4 offsets
            assert_eq!(incoming, Some(ByteOffset((i - 8) as i32 * 4)));
        }
    }

    #[test]
    fn test_ra_offset_with_tail_args() {
        // Verify RA offset accounts for tail-args
        // RA is saved at setup_area_size - 4, but setup_area_base includes tail_args
        let layout = FrameLayout::compute(&[], 0, true, 0, 10, 0, 0);

        // Setup area base is at tail_args_size (16)
        // RA is saved at setup_area_base + (setup_area_size - 4) = 16 + (8 - 4) = 20
        let setup_base = layout.setup_area_base_offset().as_i32();
        let ra_offset = setup_base + layout.setup_area_size as i32 - 4;
        assert_eq!(ra_offset, 20);
    }

    #[test]
    fn test_no_outgoing_args_but_has_frame() {
        // Test frame with no outgoing args but has other components
        let used = vec![Gpr::S0];
        let layout = FrameLayout::compute(&used, 1, true, 0, 0, 0, 0);

        // Outgoing args should still be at offset 0 (even if size is 0)
        assert_eq!(layout.outgoing_args_base_offset().as_i32(), 0);
        assert_eq!(layout.outgoing_args_size, 0);

        // Setup area should still be above (at offset 0 when no tail-args)
        assert_eq!(layout.setup_area_base_offset().as_i32(), 0);

        // Total size should not include tail-args when size is 0
        let expected_total =
            layout.setup_area_size + layout.clobber_size + layout.fixed_frame_storage_size;
        assert_eq!(layout.total_size(), expected_total);
    }
}
