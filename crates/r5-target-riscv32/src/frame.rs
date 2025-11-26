//! Frame layout computation for RISC-V 32-bit functions.
//!
//! This module implements frame layout computation aligned with Cranelift's
//! architecture. The frame layout is pre-computed before code generation.

use alloc::vec::Vec;

use riscv32_encoder::Gpr;

/// Frame layout for a RISC-V 32-bit function.
///
/// The frame layout follows Cranelift's model:
/// ```
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

        FrameLayout {
            word_bytes: 4,
            incoming_args_size,
            setup_area_size,
            clobber_size,
            fixed_frame_storage_size,
            outgoing_args_size,
            clobbered_callee_saves: used_callee_saved.to_vec(),
            has_function_calls: has_calls,
        }
    }

    /// Get the total frame size in bytes.
    pub fn total_size(&self) -> u32 {
        self.setup_area_size
            + self.clobber_size
            + self.fixed_frame_storage_size
            + self.outgoing_args_size
    }

    /// Get the stack offset for a callee-saved register.
    ///
    /// Returns the offset from SP where the register is saved.
    pub fn callee_saved_offset(&self, reg: Gpr) -> Option<i32> {
        self.clobbered_callee_saves
            .iter()
            .position(|&r| r.num() == reg.num())
            .map(|idx| {
                let offset = self.setup_area_size + (idx as u32 * 4);
                -(offset as i32)
            })
    }

    /// Get the stack offset for a spill slot.
    ///
    /// Returns the offset from SP where the spill slot is located.
    pub fn spill_slot_offset(&self, slot: u32) -> i32 {
        let base_offset = self.setup_area_size + self.clobber_size;
        -((base_offset + slot * 4) as i32)
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
        assert_eq!(offset_s0, -8);
        
        // S1 should be at offset -12 (after S0)
        let offset_s1 = layout.callee_saved_offset(Gpr::S1).unwrap();
        assert_eq!(offset_s1, -12);
    }

    #[test]
    fn test_spill_slot_offset() {
        let layout = FrameLayout::compute(&[], 2, false, 0, 0);
        
        // First spill slot should be after setup area (8 bytes)
        let offset_slot0 = layout.spill_slot_offset(0);
        assert_eq!(offset_slot0, -8);
        
        // Second spill slot should be 4 bytes after first
        let offset_slot1 = layout.spill_slot_offset(1);
        assert_eq!(offset_slot1, -12);
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
}
