mod abi;
mod compile;
mod elf;
mod emit;
mod frame;
mod liveness;
mod lower;
mod regalloc;
mod register_role;
mod spill_reload;
mod test_helpers;
mod tests;
#[cfg(feature = "debug-lowering")]
mod debug;

pub use abi::{Abi, AbiInfo};
pub use compile::{compile_module, compile_module_to_insts, CompiledModule};
pub use elf::{debug_elf, generate_elf};
pub use emit::CodeBuffer;
pub use frame::FrameLayout;
pub use liveness::{compute_liveness, LivenessInfo};
pub use lower::Lowerer;
pub use regalloc::{allocate_registers, is_callee_saved, is_caller_saved, RegisterAllocation};
pub use register_role::RegisterRole;
pub use spill_reload::{create_spill_reload_plan, SpillReloadPlan};
pub use test_helpers::{
    debug_ir, debug_ir_with_ram, expect_ir_a0, expect_ir_error, expect_ir_error_with_ram,
    expect_ir_memory_error, expect_ir_memory_error_with_ram, expect_ir_ok, expect_ir_register,
    expect_ir_syscall, expect_ir_unaligned_error,
};
