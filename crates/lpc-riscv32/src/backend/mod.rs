//! RISC-V 32-bit backend for compiling IR to machine code.

pub mod abi;
pub mod frame;
pub mod liveness;
pub mod lower;
pub mod regalloc;
pub mod spill_reload;
pub mod tests;

// Re-export for convenience
pub use abi::Abi;
pub use frame::FrameLayout;
pub use liveness::{compute_liveness, LivenessInfo};
pub use lower::Lowerer;
use lpc_lpir::Function;
pub use regalloc::{allocate_registers, RegisterAllocation};
pub use spill_reload::{create_spill_reload_plan, SpillReloadPlan};

use crate::inst_buffer::InstBuffer;

/// Compile a function from IR to RISC-V instructions.
///
/// This is the main entry point that performs the full compilation pipeline:
/// 1. Liveness analysis
/// 2. Register allocation
/// 3. Spill/reload planning
/// 4. Frame layout computation
/// 5. Instruction lowering
pub fn compile_function(func: Function) -> InstBuffer {
    // Step 1: Liveness analysis
    let liveness = compute_liveness(&func);

    // Step 2: Register allocation
    let allocation = allocate_registers(&func, &liveness);

    // Step 3: Spill/reload planning
    let spill_reload = create_spill_reload_plan(&func, &allocation, &liveness);

    // Step 4: Compute frame layout
    let num_params = func.signature.params.len();
    let num_returns = func.signature.returns.len();

    // Compute ABI info for outgoing args size
    let abi = Abi::compute_abi_info(num_params, num_returns, true);

    // Determine function call pattern (simplified - assumes no calls for now)
    // TODO: Analyze function to determine if it makes calls
    let function_calls = frame::FunctionCalls::None;

    // Calculate total spill slots needed
    let total_spill_slots = allocation.spill_slot_count + spill_reload.max_temp_spill_slots;

    // Compute frame layout
    let frame_layout = frame::compute_frame_layout(
        &allocation.used_callee_saved,
        function_calls,
        0,                        // incoming_args_size
        0,                        // tail_args_size
        total_spill_slots as u32, // stackslots_size
        0,                        // fixed_frame_storage_size (spill slots are in stackslots_size)
        abi.stack_args_size,      // outgoing_args_size
        false,                    // preserve_frame_pointers
    );

    // Step 5: Compute phi sources (needed for phi node handling)
    use lower::compute_phi_sources;
    let phi_sources = compute_phi_sources(&func, &liveness);

    // Step 6: Lower function to RISC-V instructions
    let lowerer = Lowerer::new(func, allocation, spill_reload, frame_layout, abi, liveness, phi_sources);
    lowerer.lower_function()
}
