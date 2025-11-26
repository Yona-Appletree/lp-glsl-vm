# Align Stack Args/Returns with Cranelift

## Findings from Cranelift

- RISC-V Cranelift reserves a **tail-args area** (`tail_args_size`) at the bottom of every frame that covers the largest of:
- incoming stack args for the function itself (`incoming_args_size`)
- stack space required by any callee (its `sized_stack_arg_space` a.k.a. our outgoing stack args)
- stack slots needed to hand back stack return values. They store stack returns via `StackAMode::OutgoingArg(offset + stack_arg_space)` which maps to `SP + stack_arg_space + offset`, i.e. **above the outgoing-arg area but still in the tail-args block**.
- Prologue and epilogue keep SP anchored so this tail-args region stays valid before adjusting SP back, allowing callers to load stack returns after the callee restores SP.

## Plan

0. **Tests / Validation**

- Add tests for multiple return values, small numbers (1) medium numbers (8) and large numbers (16 ensuring the stack is used).
- Add tests for nested calls with multiple return values.

1. **Introduce Tail-Args Accounting**

- Extend our `FrameLayout` to track `tail_args_size`, mirroring Cranelift.
- Compute it as `max( incoming_stack_args_size, max_outgoing_stack_args + stack_return_area )`, where `stack_return_area = align((returns > 8 ? (returns-8)*word : 0), 16)`.
- Ensure `total_size` includes `tail_args_size` first (layout: tail-args → setup → clobbers → spills) as Cranelift does.

2. **Propagate Tail-Args Metadata**

- Update ABI info / lowering entry points to keep `max_outgoing_stack_args` per function (from call graph analysis we already know `outgoing_args`); also compute `max_stack_return_area` needed by callees so callers reserve enough tail space.
- Adjust prologue/epilogue to treat tail-args as the positive-offset region at SP (no change to leaf loads before SP adjust; after prologue SP is decremented by `total_size`).

3. **Fix Return Lowering**

- When handling return values ≥8, store them at `SP + tail_args_size + offset` **before** epilogue so they end up at `caller_SP + offset` after epilogue. (Equivalently, add `tail_args_size` to the ABI-provided stack offset).
- Ensure epilogue never clobbers this region (it only adjusts SP by `total_size`).

4. **Fix Call Lowering**

- When loading stack return values, use the same ABI offsets (`(idx-8)*word`) directly from `SP` after the call returns (since tail-args area remains aligned). No extra `total_size` math should be needed if tail_args is reserved properly.
- Ensure outgoing arg stores still target the lower portion of tail-args (`SP + (idx-8)*word`).
