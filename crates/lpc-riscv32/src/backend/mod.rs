//! RISC-V 32-bit backend for compiling IR to machine code.

pub mod abi;
pub mod frame;
pub mod liveness;
pub mod lower;
pub mod regalloc;
pub mod spill_reload;
pub mod test_helpers;
pub mod tests;

// Re-export for convenience
use alloc::{collections::BTreeMap, string::String, vec::Vec};

pub use abi::Abi;
pub use frame::FrameLayout;
pub use liveness::{compute_liveness, LivenessInfo};
pub use lower::Lowerer;
use lpc_lpir::{Function, Module};
pub use regalloc::{allocate_registers, RegisterAllocation};
pub use spill_reload::{create_spill_reload_plan, SpillReloadPlan};
pub use test_helpers::{
    debug_ir, debug_ir_with_ram, expect_ir_a0, expect_ir_error, expect_ir_error_with_ram,
    expect_ir_memory_error, expect_ir_memory_error_with_ram, expect_ir_ok, expect_ir_register,
    expect_ir_syscall, expect_ir_unaligned_error,
};

use crate::{inst_buffer::InstBuffer, Inst};

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
    let lowerer = Lowerer::new(
        func,
        allocation,
        spill_reload,
        frame_layout,
        abi,
        liveness,
        phi_sources,
    );
    lowerer.lower_function().0 // Return just the instruction buffer (call relocs handled at module level)
}

/// A compiled module containing code for all functions.
pub struct CompiledModule {
    /// Combined instruction buffer for all functions
    pub code: InstBuffer,
    /// Size of the bootstrap/entry function in instructions
    pub bootstrap_size: usize,
}

impl CompiledModule {
    /// Convert the compiled code to bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        Ok(self.code.as_bytes())
    }
}

/// Compile a module containing multiple functions to RISC-V instructions.
///
/// This compiles all functions in the module and combines them into a single
/// instruction buffer. The entry function is placed first.
pub fn compile_module_to_insts(module: &Module) -> Result<CompiledModule, String> {
    let mut combined_code = InstBuffer::new();
    let mut bootstrap_size = 0;
    let mut function_addresses: BTreeMap<String, usize> = BTreeMap::new();
    let mut all_call_relocations: Vec<(usize, String)> = Vec::new();

    // Get entry function name
    let entry_name = module
        .entry_function
        .as_ref()
        .ok_or_else(|| String::from("Module does not have an entry function"))?;

    // First pass: Compile all functions to get their code and relocations
    // We compile all functions first so we know their sizes before emitting
    let mut function_buffers: BTreeMap<String, (InstBuffer, Vec<lower::CallRelocation>)> =
        BTreeMap::new();

    // Compile entry function
    let entry_func = module
        .get_function(entry_name)
        .ok_or_else(|| alloc::format!("Entry function '{}' not found in module", entry_name))?;

    let entry_liveness = compute_liveness(entry_func);
    let entry_allocation = allocate_registers(entry_func, &entry_liveness);
    let entry_spill_reload =
        create_spill_reload_plan(entry_func, &entry_allocation, &entry_liveness);
    let entry_num_params = entry_func.signature.params.len();
    let entry_num_returns = entry_func.signature.returns.len();
    let entry_abi = Abi::compute_abi_info(entry_num_params, entry_num_returns, true);
    let entry_has_calls = entry_func.blocks.iter().any(|block| {
        block
            .insts
            .iter()
            .any(|inst| matches!(inst, lpc_lpir::Inst::Call { .. }))
    });
    let entry_function_calls = if entry_has_calls {
        frame::FunctionCalls::Regular
    } else {
        frame::FunctionCalls::None
    };
    let entry_total_spill_slots =
        entry_allocation.spill_slot_count + entry_spill_reload.max_temp_spill_slots;
    let entry_frame_layout = frame::compute_frame_layout(
        &entry_allocation.used_callee_saved,
        entry_function_calls,
        0,
        0,
        entry_total_spill_slots as u32,
        0,
        entry_abi.stack_args_size,
        false,
    );
    let entry_phi_sources = lower::compute_phi_sources(entry_func, &entry_liveness);
    let entry_lowerer = Lowerer::new(
        entry_func.clone(),
        entry_allocation,
        entry_spill_reload,
        entry_frame_layout,
        entry_abi,
        entry_liveness,
        entry_phi_sources,
    );
    let (entry_code, entry_call_relocs) = entry_lowerer.lower_function();
    function_buffers.insert(entry_name.clone(), (entry_code, entry_call_relocs));
    bootstrap_size = function_buffers[entry_name].0.instruction_count();

    // Compile remaining functions
    for (name, func) in &module.functions {
        if name != entry_name {
            let func_liveness = compute_liveness(func);
            let func_allocation = allocate_registers(func, &func_liveness);
            let func_spill_reload =
                create_spill_reload_plan(func, &func_allocation, &func_liveness);
            let func_num_params = func.signature.params.len();
            let func_num_returns = func.signature.returns.len();
            let func_abi = Abi::compute_abi_info(func_num_params, func_num_returns, true);
            let func_has_calls = func.blocks.iter().any(|block| {
                block
                    .insts
                    .iter()
                    .any(|inst| matches!(inst, lpc_lpir::Inst::Call { .. }))
            });
            let func_function_calls = if func_has_calls {
                frame::FunctionCalls::Regular
            } else {
                frame::FunctionCalls::None
            };
            let func_total_spill_slots =
                func_allocation.spill_slot_count + func_spill_reload.max_temp_spill_slots;
            let func_frame_layout = frame::compute_frame_layout(
                &func_allocation.used_callee_saved,
                func_function_calls,
                0,
                0,
                func_total_spill_slots as u32,
                0,
                func_abi.stack_args_size,
                false,
            );
            let func_phi_sources = lower::compute_phi_sources(func, &func_liveness);
            let func_lowerer = Lowerer::new(
                func.clone(),
                func_allocation,
                func_spill_reload,
                func_frame_layout,
                func_abi,
                func_liveness,
                func_phi_sources,
            );
            let (func_code, func_call_relocs) = func_lowerer.lower_function();
            function_buffers.insert(name.clone(), (func_code, func_call_relocs));
        }
    }

    // Second pass: Emit all functions and record their final addresses
    // Entry function first
    let entry_start = combined_code.instruction_count();
    function_addresses.insert(entry_name.clone(), entry_start);
    let (entry_code, entry_call_relocs) = &function_buffers[entry_name];
    #[cfg(test)]
    {
        extern crate std;
        std::println!(
            "Emitting {} at {}, {} instructions",
            entry_name,
            entry_start,
            entry_code.instruction_count()
        );
    }
    for reloc in entry_call_relocs {
        all_call_relocations.push((entry_start + reloc.inst_idx, reloc.callee_name.clone()));
    }
    for inst in entry_code.instructions() {
        combined_code.emit(inst.clone());
    }

    // Then remaining functions (in alphabetical order from BTreeMap)
    for (name, _func) in &module.functions {
        if name != entry_name {
            let func_start = combined_code.instruction_count();
            function_addresses.insert(name.clone(), func_start);
            let (func_code, func_call_relocs) = &function_buffers[name];
            #[cfg(test)]
            {
                extern crate std;
                std::println!(
                    "Emitting {} at {}, {} instructions",
                    name,
                    func_start,
                    func_code.instruction_count()
                );
            }
            for reloc in func_call_relocs {
                all_call_relocations.push((func_start + reloc.inst_idx, reloc.callee_name.clone()));
            }
            for inst in func_code.instructions() {
                combined_code.emit(inst.clone());
            }
        }
    }

    // Fix up all function call relocations
    for (call_inst_idx, callee_name) in all_call_relocations {
        let target_addr = function_addresses
            .get(&callee_name)
            .ok_or_else(|| alloc::format!("Function '{}' not found in module", callee_name))?;

        // Calculate PC-relative offset (in bytes)
        // JAL offset is PC-relative in BYTES, not instructions
        // PC at JAL instruction = call_inst_idx * 4 (bytes)
        // Target = target_addr * 4 (bytes)
        // Offset in bytes = (target_addr - call_inst_idx) * 4
        let offset_bytes = ((*target_addr as i32) - (call_inst_idx as i32)) * 4;

        #[cfg(test)]
        {
            extern crate std;
            std::println!(
                "Reloc: call_inst_idx={}, callee={}, target_addr={}, offset_bytes={}, \
                 PC=0x{:04x}, target=0x{:04x}",
                call_inst_idx,
                callee_name,
                target_addr,
                offset_bytes,
                call_inst_idx * 4,
                target_addr * 4
            );
            // Print all function addresses for debugging
            std::println!("Function addresses: {:?}", function_addresses);
        }

        // Get current instruction and update offset
        let insts = combined_code.instructions();
        let current_inst = &insts[call_inst_idx];

        let fixed_inst = match current_inst {
            Inst::Jal { rd, .. } => {
                #[cfg(test)]
                {
                    extern crate std;
                    std::println!(
                        "Setting JAL at idx {}: old_imm={:?}, new_imm={} bytes",
                        call_inst_idx,
                        current_inst,
                        offset_bytes
                    );
                }
                Inst::Jal {
                    rd: *rd,
                    imm: offset_bytes,
                }
            }
            _ => {
                return Err(alloc::format!(
                    "Call relocation at instruction {} is not a JAL instruction",
                    call_inst_idx
                ));
            }
        };

        combined_code.set_instruction(call_inst_idx, fixed_inst);

        // Verify it was set correctly
        #[cfg(test)]
        {
            extern crate std;
            let verify_inst = &combined_code.instructions()[call_inst_idx];
            std::println!(
                "After set: inst at idx {} = {:?}",
                call_inst_idx,
                verify_inst
            );
        }
    }

    Ok(CompiledModule {
        code: combined_code,
        bootstrap_size,
    })
}
