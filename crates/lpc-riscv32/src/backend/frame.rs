//! Stack frame layout computation for RISC-V 32-bit.

extern crate alloc;

use alloc::vec::Vec;

use crate::Gpr;

/// Classification of function call patterns.
///
/// This is used to determine what calling convention support is needed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FunctionCalls {
    /// Function makes no calls at all.
    #[default]
    None,
    /// Function only makes tail calls (no regular calls).
    TailOnly,
    /// Function makes at least one regular call (may also have tail calls).
    Regular,
}

impl FunctionCalls {
    /// Update the function classification based on a new call instruction.
    pub fn update(&mut self, call_type: CallType) {
        *self = match (*self, call_type) {
            (current, CallType::None) => current,
            (_, CallType::Regular) => FunctionCalls::Regular,
            (FunctionCalls::None, CallType::TailCall) => FunctionCalls::TailOnly,
            (current, CallType::TailCall) => current,
        };
    }
}

/// Type of call instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallType {
    /// Not a call instruction.
    None,
    /// Regular function call.
    Regular,
    /// Tail call.
    TailCall,
}

/// Structure describing the layout of a function's stack frame.
///
/// This follows the RISC-V 32-bit ABI specification. The frame layout
/// describes how stack space is organized from high to low addresses:
///
/// 1. Incoming arguments (in caller's frame)
/// 2. Setup area (FP + RA) - 8 bytes
/// 3. Clobber area (callee-saved registers)
/// 4. Fixed frame storage (spill slots, etc.)
/// 5. Outgoing arguments (for calls made by this function)
#[derive(Clone, Debug, Default)]
pub struct FrameLayout {
    /// Word size in bytes (4 for RV32).
    pub word_bytes: u32,

    /// Size of incoming arguments on the stack.
    /// This is not technically part of this function's frame, but code
    /// in the function will need to access it.
    pub incoming_args_size: u32,

    /// The size of the incoming argument area, taking into account any
    /// potential increase in size required for tail calls present in the
    /// function. In the case that no tail calls are present, this value
    /// will be the same as `incoming_args_size`.
    pub tail_args_size: u32,

    /// Size of the "setup area", holding the return address and saved
    /// frame pointer. This is 8 bytes for RV32 (FP at offset 0, RA at offset 4).
    pub setup_area_size: u32,

    /// Size of the area used to save callee-saved clobbered registers.
    /// This area is aligned to 16 bytes.
    pub clobber_size: u32,

    /// Storage allocated for the fixed part of the stack frame.
    /// This contains stack slots and spill slots.
    pub fixed_frame_storage_size: u32,

    /// The size of all stackslots.
    pub stackslots_size: u32,

    /// Stack size to be reserved for outgoing arguments.
    /// After gen_clobber_save and before gen_clobber_restore, the stack
    /// pointer points to the bottom of this area.
    pub outgoing_args_size: u32,

    /// Sorted list of callee-saved registers that are clobbered.
    /// These registers will be saved and restored by gen_clobber_save
    /// and gen_clobber_restore.
    pub clobbered_callee_saves: Vec<Gpr>,

    /// The function's call pattern classification.
    pub function_calls: FunctionCalls,
}

impl FrameLayout {
    /// The size of FP to SP while the frame is active (not during prologue
    /// setup or epilogue tear down).
    pub fn active_size(&self) -> u32 {
        self.outgoing_args_size + self.fixed_frame_storage_size + self.clobber_size
    }

    /// Get the offset from the SP to the sized stack slots area.
    pub fn sp_to_sized_stack_slots(&self) -> u32 {
        self.outgoing_args_size
    }

    /// Get the offset from SP up to FP.
    pub fn sp_to_fp(&self) -> u32 {
        self.outgoing_args_size + self.fixed_frame_storage_size + self.clobber_size
    }
}

/// Callee-saved registers for RISC-V 32-bit.
///
/// These are: x8 (s0/fp), x9 (s1), x18-x27 (s2-s11)
const CALLEE_SAVED: &[Gpr] = &[
    Gpr::S0,  // x8 - frame pointer
    Gpr::S1,  // x9
    Gpr::S2,  // x18
    Gpr::S3,  // x19
    Gpr::S4,  // x20
    Gpr::S5,  // x21
    Gpr::S6,  // x22
    Gpr::S7,  // x23
    Gpr::S8,  // x24
    Gpr::S9,  // x25
    Gpr::S10, // x26
    Gpr::S11, // x27
];

/// Compute the size needed for clobbered callee-saved registers.
///
/// Each integer register is 4 bytes. Registers are stored with proper
/// alignment. The total size is aligned to 16 bytes.
fn compute_clobber_size(regs: &[Gpr]) -> u32 {
    if regs.is_empty() {
        return 0;
    }

    // Each register is 4 bytes (RV32)
    let total_size = regs.len() as u32 * 4;

    // Align to 16 bytes (RISC-V ABI requirement)
    (total_size + 15) & !15
}

/// Compute the frame layout for a function.
///
/// This follows cranelift's `compute_frame_layout` logic, adapted for RV32.
///
/// # Parameters
///
/// - `regs`: List of registers that may be clobbered (will be filtered to callee-saved)
/// - `function_calls`: Classification of call patterns in the function
/// - `incoming_args_size`: Size of incoming arguments on the stack
/// - `tail_args_size`: Size of incoming args accounting for tail calls
/// - `stackslots_size`: Size of stack slots
/// - `fixed_frame_storage_size`: Size of fixed frame storage
/// - `outgoing_args_size`: Size needed for outgoing arguments
/// - `preserve_frame_pointers`: Whether frame pointers should be preserved
pub fn compute_frame_layout(
    regs: &[Gpr],
    function_calls: FunctionCalls,
    incoming_args_size: u32,
    tail_args_size: u32,
    stackslots_size: u32,
    fixed_frame_storage_size: u32,
    outgoing_args_size: u32,
    preserve_frame_pointers: bool,
) -> FrameLayout {
    // Filter to only callee-saved registers
    let mut callee_saved_regs: Vec<Gpr> = regs
        .iter()
        .filter(|reg| CALLEE_SAVED.contains(reg))
        .copied()
        .collect();

    // Sort for consistent ordering
    callee_saved_regs.sort_by_key(|r| r.num());

    // Compute clobber size
    let clobber_size = compute_clobber_size(&callee_saved_regs);

    // Compute setup area size
    // Setup area is needed if:
    // - Frame pointers are preserved, OR
    // - Function makes calls, OR
    // - There are incoming stack arguments (need FP to access them), OR
    // - There are clobbered registers to save, OR
    // - There is fixed frame storage
    let setup_area_size = if preserve_frame_pointers
        || function_calls != FunctionCalls::None
        || incoming_args_size > 0
        || clobber_size > 0
        || fixed_frame_storage_size > 0
    {
        8 // FP (4 bytes) + RA (4 bytes) for RV32
    } else {
        0
    };

    FrameLayout {
        word_bytes: 4, // RV32
        incoming_args_size,
        tail_args_size,
        setup_area_size,
        clobber_size,
        fixed_frame_storage_size,
        stackslots_size,
        outgoing_args_size,
        clobbered_callee_saves: callee_saved_regs,
        function_calls,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_frame_no_calls() {
        let layout = compute_frame_layout(&[], FunctionCalls::None, 0, 0, 0, 0, 0, false);

        assert_eq!(layout.setup_area_size, 0);
        assert_eq!(layout.clobber_size, 0);
        assert_eq!(layout.word_bytes, 4);
    }

    #[test]
    fn test_frame_with_calls() {
        let layout = compute_frame_layout(&[], FunctionCalls::Regular, 0, 0, 0, 0, 0, false);

        assert_eq!(layout.setup_area_size, 8);
        assert_eq!(layout.clobber_size, 0);
    }

    #[test]
    fn test_frame_with_clobbered_registers() {
        let layout = compute_frame_layout(
            &[Gpr::S1, Gpr::S2],
            FunctionCalls::Regular,
            0,
            0,
            0,
            0,
            0,
            false,
        );

        assert_eq!(layout.setup_area_size, 8);
        // 2 registers * 4 bytes = 8 bytes, aligned to 16 = 16 bytes
        assert_eq!(layout.clobber_size, 16);
        assert_eq!(layout.clobbered_callee_saves.len(), 2);
    }

    #[test]
    fn test_frame_with_incoming_args() {
        let layout = compute_frame_layout(
            &[],
            FunctionCalls::None,
            16, // 4 arguments on stack
            16,
            0,
            0,
            0,
            false,
        );

        assert_eq!(layout.setup_area_size, 8); // Needed to access stack args
        assert_eq!(layout.incoming_args_size, 16);
    }

    #[test]
    fn test_frame_with_outgoing_args() {
        let layout = compute_frame_layout(
            &[],
            FunctionCalls::Regular,
            0,
            0,
            0,
            0,
            32, // Space for outgoing args
            false,
        );

        assert_eq!(layout.setup_area_size, 8);
        assert_eq!(layout.outgoing_args_size, 32);
    }

    #[test]
    fn test_clobber_size_alignment() {
        // Test that clobber size is properly aligned
        let layout1 =
            compute_frame_layout(&[Gpr::S1], FunctionCalls::Regular, 0, 0, 0, 0, 0, false);
        // 1 register = 4 bytes, aligned to 16 = 16 bytes
        assert_eq!(layout1.clobber_size, 16);

        let layout2 = compute_frame_layout(
            &[Gpr::S1, Gpr::S2, Gpr::S3, Gpr::S4],
            FunctionCalls::Regular,
            0,
            0,
            0,
            0,
            0,
            false,
        );
        // 4 registers = 16 bytes, aligned to 16 = 16 bytes
        assert_eq!(layout2.clobber_size, 16);

        let layout3 = compute_frame_layout(
            &[Gpr::S1, Gpr::S2, Gpr::S3, Gpr::S4, Gpr::S5],
            FunctionCalls::Regular,
            0,
            0,
            0,
            0,
            0,
            false,
        );
        // 5 registers = 20 bytes, aligned to 16 = 32 bytes
        assert_eq!(layout3.clobber_size, 32);
    }
}
