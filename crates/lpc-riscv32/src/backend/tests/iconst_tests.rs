#[cfg(test)]
mod tests {
    extern crate std;

    use lpc_lpir::parse_function;

    use crate::backend::{
        allocate_registers, compute_liveness, create_spill_reload_plan, Abi, Lowerer,
    };

    #[test]
    fn test_lower_iconst() {
        // Function with iconst
        let ir = r#"
function %test() -> i32 {
block0:
    v0 = iconst 42
    return v0
}"#;

        use crate::backend::{
            frame::{compute_frame_layout, FunctionCalls},
            lower::compute_phi_sources,
        };

        let func = parse_function(ir).expect("Failed to parse IR function");
        let liveness = compute_liveness(&func);
        let allocation = allocate_registers(&func, &liveness);
        let spill_reload = create_spill_reload_plan(&func, &allocation, &liveness);

        let total_spill_slots = allocation.spill_slot_count + spill_reload.max_temp_spill_slots;

        let num_params = func.signature.params.len();
        let num_returns = func.signature.returns.len();
        let abi = Abi::compute_abi_info(num_params, num_returns, true);

        let frame_layout = compute_frame_layout(
            &allocation.used_callee_saved,
            FunctionCalls::None,
            0,                        // incoming_args_size
            0,                        // tail_args_size
            total_spill_slots as u32, // stackslots_size
            0,                        // fixed_frame_storage_size
            abi.stack_args_size,      // outgoing_args_size
            false,                    // preserve_frame_pointers
        );

        let phi_sources = compute_phi_sources(&func, &liveness);

        let lowerer = Lowerer::new(
            func.clone(),
            allocation,
            spill_reload,
            frame_layout,
            abi,
            liveness,
            phi_sources,
        );
        let code = lowerer.lower_function().0; // Return just the instruction buffer

        assert!(code.instruction_count() > 0);
    }
}
