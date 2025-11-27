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
use lpc_lpir::{Function, Module};
pub use regalloc::{allocate_registers, RegisterAllocation};
pub use spill_reload::{create_spill_reload_plan, SpillReloadPlan};

use crate::inst_buffer::InstBuffer;
use alloc::{string::String, vec::Vec};

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
    use alloc::collections::BTreeMap;

    let mut combined_code = InstBuffer::new();
    let mut bootstrap_size = 0;

    // Get entry function name
    let entry_name = module.entry_function.as_ref().ok_or_else(|| {
        String::from("Module does not have an entry function")
    })?;

    // Compile entry function first
    let entry_func = module.get_function(entry_name).ok_or_else(|| {
        alloc::format!("Entry function '{}' not found in module", entry_name)
    })?;
    let entry_code = compile_function(entry_func.clone());
    bootstrap_size = entry_code.instruction_count();
    for inst in entry_code.instructions() {
        combined_code.emit(inst.clone());
    }

    // Compile remaining functions
    for (name, func) in &module.functions {
        if name != entry_name {
            let func_code = compile_function(func.clone());
            for inst in func_code.instructions() {
                combined_code.emit(inst.clone());
            }
        }
    }

    Ok(CompiledModule {
        code: combined_code,
        bootstrap_size,
    })
}

/// Expect IR code to run and produce a specific value in register a0.
///
/// The IR code should be a module with an entry function that eventually
/// halts, leaving the result in a0.
///
/// This is a test helper function.
pub fn expect_ir_a0(ir: &str, expected: i32) {
    use crate::Riscv32Emulator;
    use crate::elf::generate_elf;
    use lpc_lpir::parse_module;

    let module = parse_module(ir).expect("Failed to parse IR module");
    let compiled = compile_module_to_insts(&module).expect("Failed to compile module");
    let bytes = compiled.to_bytes().expect("Failed to convert to bytes");
    let elf = generate_elf(&bytes);

    let mut emu = Riscv32Emulator::new(elf, alloc::vec![0; 1024 * 1024]);
    match emu.run_until_ebreak() {
        Ok(_) => {
            let actual = emu.get_register(crate::Gpr::A0);
            if actual != expected {
                panic!(
                    "Register a0 mismatch: expected {}, got {}\n\nIR:\n{}",
                    expected, actual, ir
                );
            }
        }
        Err(e) => {
            panic!("Execution error: {}\n\nIR:\n{}", e, ir);
        }
    }
}

/// Expect IR code to run successfully until EBREAK, returning the emulator.
///
/// This is a test helper function.
pub fn expect_ir_ok(ir: &str) -> crate::Riscv32Emulator {
    use crate::Riscv32Emulator;
    use crate::elf::generate_elf;
    use lpc_lpir::parse_module;

    let module = parse_module(ir).expect("Failed to parse IR module");
    let compiled = compile_module_to_insts(&module).expect("Failed to compile module");
    let bytes = compiled.to_bytes().expect("Failed to convert to bytes");
    let elf = generate_elf(&bytes);

    let mut emu = Riscv32Emulator::new(elf, alloc::vec![0; 1024 * 1024]);
    match emu.run_until_ebreak() {
        Ok(_) => emu,
        Err(e) => {
            panic!("Execution error: {}\n\nIR:\n{}", e, ir);
        }
    }
}

/// Expect IR code to run until a syscall and verify the syscall info.
///
/// Returns the emulator after the syscall for further inspection.
///
/// This is a test helper function.
pub fn expect_ir_syscall(ir: &str, expected_syscall: i32, expected_args: &[i32]) -> crate::Riscv32Emulator {
    use crate::{Riscv32Emulator, StepResult};
    use crate::elf::generate_elf;
    use lpc_lpir::parse_module;

    let module = parse_module(ir).expect("Failed to parse IR module");
    let compiled = compile_module_to_insts(&module).expect("Failed to compile module");
    let bytes = compiled.to_bytes().expect("Failed to convert to bytes");
    let elf = generate_elf(&bytes);

    let mut emu = Riscv32Emulator::new(elf, alloc::vec![0; 1024 * 1024]);
    loop {
        match emu.step() {
            Ok(StepResult::Syscall(syscall_info)) => {
                if syscall_info.number != expected_syscall {
                    panic!(
                        "Syscall number mismatch: expected {}, got {}\n\nIR:\n{}",
                        expected_syscall, syscall_info.number, ir
                    );
                }
                if syscall_info.args.len() != expected_args.len() {
                    panic!(
                        "Syscall args count mismatch: expected {}, got {}\n\nIR:\n{}",
                        expected_args.len(), syscall_info.args.len(), ir
                    );
                }
                for (i, (actual, expected)) in syscall_info.args.iter().zip(expected_args.iter()).enumerate() {
                    if *actual != *expected {
                        panic!(
                            "Syscall arg[{}] mismatch: expected {}, got {}\n\nIR:\n{}",
                            i, expected, actual, ir
                        );
                    }
                }
                return emu;
            }
            Ok(StepResult::Halted) => {
                panic!("Program halted before syscall\n\nIR:\n{}", ir);
            }
            Ok(_) => {
                // Continue execution
            }
            Err(e) => {
                panic!("Execution error: {}\n\nIR:\n{}", e, ir);
            }
        }
    }
}
