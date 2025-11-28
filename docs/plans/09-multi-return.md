## Tail-Args + Multi-Return Fix Plan

### 1. Current State

- **Failing tests** (all under `crates/lpc-codegen`):
  - `backend::lower::call::tests::test_multiple_returns_medium` (expects 36, gets 33)
  - `backend::lower::call::tests::test_nested_calls_with_multiple_returns` (expects 315, gets 310)
  - `backend::lower::call::tests::test_large_function_stress_test` (expects 272, gets 2)
- `FrameLayout::total_size()` currently lumps `tail_args_size` together with the callee’s own frame (setup/clobber/spill areas). This causes offset math in prologue/epilogue/call/return lowering to intermix caller-reserved tail space with callee-reserved storage.
- Manual inline math has proliferated across `lower/call.rs`, `lower/return_.rs`, `lower/prologue.rs`, and `lower/epilogue.rs`, making it easy to mix caller vs callee offsets.

### 2. Reference Implementation (Cranelift)

- Source: `/Users/yona/dev/photomancer/wasmtime/cranelift/codegen/src/isa/riscv64/abi.rs`.
- Cranelift models a **tail-args block** that remains valid for the caller across prologue/epilogue. It tracks:
  - `incoming_args_size`
  - `outgoing_args_size`
  - `stack_return_area`
  - `max_callee_stack_return_area`
  - `tail_args_size = max(incoming, outgoing + callee ret, self ret)`
- Cranelift centralizes stack math via `FrameLayout` helpers (e.g., `StackAMode::OutgoingArg(offset + stack_arg_space)`), so lowering code never re-derives offsets.
- During clobber save they adjust SP by `tail_args_size - incoming_args_size` _before_ touching locals, ensuring a single canonical view of tail space, and tail arguments are kept separate from spill/clobber areas.

### 3. Goals

1. **Match Cranelift’s tail-args semantics** so outgoing stack args + stack return slots live in a persistent caller area, while callee-local storage remains isolated.
2. **Centralize all stack math** in `FrameLayout`:
   - Provide helpers for: incoming args, outgoing args, stack returns, RA save slot, spills, callee-saved regs, etc.
   - Lowering code should stop recomputing `SP + N`; instead, request a `ByteOffset`/`StackSlot` descriptor from `FrameLayout`.
3. **Clarify ownership of tail-args**:
   - Callers reserve `tail_args_size`.
   - Callees may need additional incoming space if they accept more stack args than caller provided (`tail_args_size - incoming_args_size` in Cranelift).
4. **Unify ABI metadata** so `AbiInfo` exposes only high-level facts (arg/ret register mapping, indices, counts). All concrete offsets come from `FrameLayout`.
5. **Add/restore tests** for the multi-return scenarios (8+ returns, nested callers, 16-return stress) to verify stack return handling.

### 4. Detailed Work Plan

#### Step A – Frame Layout Refactor (`crates/lpc-codegen/src/backend/frame.rs`)

1. Revisit `FrameLayout::compute` to separate caller-owned tail space from callee-owned areas:
   - Track `tail_args_size` (as today) but expose **two derived values**:
     - `caller_tail_size` (what must already exist when this function is invoked = max incoming stack args, stack return area for this fn).
     - `extra_tail_needed` (additional tail space the caller must reserve so this function can in turn pass stack args / stack returns to callees).
   - Possibly follow Cranelift exactly: `tail_args_size = max(incoming_args_size, outgoing_args_size + max_callee_stack_return_area, stack_return_area)` but store `incoming_args_size` separately so we know how much more needs to be subtracted from SP.
2. Provide explicit helpers:
   - `outgoing_stack_arg_offset(idx)` → `ByteOffset`
   - `incoming_stack_arg_offset(idx)`
   - `stack_return_offset(idx)` (for storing >=8th return before epilogue)
   - `callee_return_load_offset(idx)` (for caller to load stack returns after call)
   - `tail_args_adjustment_for_prologue()` (how much SP delta is required so tail space is consistent with caller expectations)
3. Ensure `total_size_without_tail_args()` and `total_frame_adjustment()` are distinct. Prologue should adjust SP by:
   - `tail_adjustment = tail_args_size - incoming_args_size` (if positive) _before_ saving RA/Fp.
   - `local_frame_size = setup + clobber + spills`.
4. Write doc-comments describing the final stack picture and include ASCII art similar to Cranelift.
5. Update existing unit tests (e.g., `test_total_size_includes_tail_args`) to reflect the new accounting—tests should assert tail area values via the helper functions rather than by recomputing arithmetic inline.

#### Step B – ABI Data Flow (`crates/lpc-codegen/src/backend/abi.rs`, `backend/compile.rs`)

1. Keep `AbiInfo` focused on register vs stack classification; remove/prevent direct offset math.
2. Propagate the maximum stack return requirement per callee (already tracked) into `FrameLayout`.
3. Ensure `compile::compute_max_outgoing_args` and `compute_max_callee_stack_returns` feed into `FrameLayout` so tail sizing is correct for every function.

#### Step C – Prologue/Epilogue Updates (`lower/prologue.rs`, `lower/epilogue.rs`)

1. Replace inline `SP + imm` expressions with new `FrameLayout` helpers:
   - Loading incoming stack args before the prologue should use `incoming_stack_arg_offset`.
   - Saving/restoring RA/callee-saved registers should use helper-provided offsets.
2. Implement the two-phase SP adjustment:
   - Phase 1: ensure enough tail space (`tail_adjustment`).
   - Phase 2: allocate local frame (`local_frame_size`).
     This mirrors Cranelift’s `gen_clobber_save` logic.
3. Epilogue must reverse the order (restore locals, deallocate local frame, then undo any tail adjustment if we performed one).

#### Step D – Call Lowering (`lower/call.rs`)

1. For each arg/result index ≥ 8, query `FrameLayout` for the correct offset rather than re-deriving `(idx-8)*4`.
2. Outgoing stack args:
   - Use `frame_layout.outgoing_stack_arg_offset(idx)` to decide where to store arguments relative to the caller’s SP.
3. Stack return loads:
   - Call-site loads must reference the tail-args helper (e.g., `tail_stack_return_offset(idx)`), ensuring they look above outgoing args if needed.
4. Remove all ad-hoc printing of frame_size/outgoing_args_size once the helpers encapsulate that logic.

#### Step E – Return Lowering (`lower/return_.rs`)

1. Store stack returns (idx ≥ 8) at the offset provided by the helper (something like `frame_layout.stack_return_store_offset(idx)`).
2. After adjusting SP to exit the function, these values must line up with the caller’s expected load offset. Assert (during debug builds) that store/load offsets align.

#### Step F – Tests & Validation

1. Re-enable and extend existing call tests:
   - `test_multiple_returns_medium`
   - `test_nested_calls_with_multiple_returns`
   - `test_large_function_stress_test`
2. Add new tests explicitly covering:
   - Caller with fewer incoming stack args than callee outgoing requirements (ensures tail adjustment works).
   - Functions returning >8 values where the caller also keeps live stack args.
3. Add targeted unit tests inside `frame.rs` validating each helper (incoming/outgoing/return offsets, tail adjustments) for several layouts (no stack args, stack args only, large stack returns).
4. Once code changes are in place, run `just all` to cover formatting, clippy, build, and tests.

### 5. Open Questions / Follow-ups

- Should `FrameLayout` expose a structured enum describing stack regions (e.g., `FrameRegion::TailArgs`, `FrameRegion::Setup`, etc.) to simplify debugging/logging?
- Do we need to support tail-call lowering soon? If yes, crib additional logic from Cranelift’s handling (`call_conv == CallConv::Tail` adjustments).
- Investigate whether we want to gate the extra SP adjustment on `has_calls || outgoing_args_size > incoming_args_size`; copying Cranelift exactly might be simplest.

This plan keeps all stack arithmetic inside `FrameLayout`, aligns our ABI semantics with Cranelift, and provides a roadmap for fixing the current multi-return regressions without yet touching executable code.


