# RISC-V 64-bit Call Frame Layout

Based on code analysis of `/Users/yona/dev/photomancer/wasmtime/cranelift/codegen/src/isa/riscv64/abi.rs`

## Register Usage

- **SP (Stack Pointer)**: x2 (`stack_reg()`)
- **FP (Frame Pointer)**: x8 (`fp_reg()`)
- **RA/LR (Return Address)**: x1 (`link_reg()`)

## Stack Frame Layout (Stack Grows Downward)

```
High Addresses (Caller's Frame)
┌─────────────────────────────────────────┐
│                                         │
│         Caller's Stack Frame            │
│                                         │
└─────────────────────────────────────────┘
                    ▲
                    │ Stack grows downward
                    │ (toward lower addresses)
                    │
┌─────────────────────────────────────────┐
│ FP+8  │ Return Address (RA/x1)         │ ← Saved by callee
├───────┼─────────────────────────────────┤
│ FP+0  │ Old Frame Pointer (FP/x8)      │ ← Saved by callee
├───────┼─────────────────────────────────┤
│       │                                 │
│       │  Setup Area (16 bytes)          │ ← frame_layout.setup_area_size
│       │  - Only allocated if:           │
│       │    * preserve_frame_pointers()  │
│       │    * function_calls != None     │
│       │    * incoming_args_size > 0      │
│       │    * clobber_size > 0           │
│       │    * fixed_frame_storage_size>0 │
│       │                                 │
├───────┼─────────────────────────────────┤
│       │                                 │
│       │  Clobber Area                   │ ← frame_layout.clobber_size
│       │  (Callee-saved registers)       │   (aligned to 16 bytes)
│       │  - Stored from top downward    │
│       │  - Includes: x9, x18-x27       │
│       │             f8-f9, f18-f27      │
│       │  - Each register: 8 bytes       │
│       │  - Vector regs: 16 bytes        │
│       │                                 │
├───────┼─────────────────────────────────┤
│       │                                 │
│       │  Fixed Frame Storage            │ ← frame_layout.fixed_frame_storage_size
│       │  (Fixed storage slots)          │
│       │                                 │
├───────┼─────────────────────────────────┤
│       │                                 │
│       │  Stack Slots                    │ ← frame_layout.stackslots_size
│       │  (Spill slots for registers)    │
│       │                                 │
├───────┼─────────────────────────────────┤
│       │                                 │
│       │  Outgoing Arguments             │ ← frame_layout.outgoing_args_size
│       │  (Space for arguments to        │
│       │   called functions)              │
│       │                                 │
└───────┴─────────────────────────────────┘
        │
        ▼ SP (Stack Pointer) - Bottom of current frame
```

## Frame Layout Structure (from code)

From `compute_frame_layout()` (lines 630-681):

- **setup_area_size**: 16 bytes (FP + LR) or 0
- **clobber_size**: Space for callee-saved registers that are clobbered
- **fixed_frame_storage_size**: Fixed storage slots
- **stackslots_size**: Spill slots for register spills
- **outgoing_args_size**: Space for outgoing arguments

## Prologue Sequence (from `gen_prologue_frame_setup`)

```338:369:cranelift/codegen/src/isa/riscv64/abi.rs
        if frame_layout.setup_area_size > 0 {
            // add  sp,sp,-16    ;; alloc stack space for fp.
            // sd   ra,8(sp)     ;; save ra.
            // sd   fp,0(sp)     ;; store old fp.
            // mv   fp,sp        ;; set fp to sp.
            insts.extend(Self::gen_sp_reg_adjust(-16));
            insts.push(Inst::gen_store(
                AMode::SPOffset(8),
                link_reg(),
                I64,
                MemFlags::trusted(),
            ));
            insts.push(Inst::gen_store(
                AMode::SPOffset(0),
                fp_reg(),
                I64,
                MemFlags::trusted(),
            ));

            if flags.unwind_info() {
                insts.push(Inst::Unwind {
                    inst: UnwindInst::PushFrameRegs {
                        offset_upward_to_caller_sp: frame_layout.setup_area_size,
                    },
                });
            }
            insts.push(Inst::Mov {
                rd: writable_fp_reg(),
                rm: stack_reg(),
                ty: I64,
            });
        }
```

## Clobber Save Sequence (from `gen_clobber_save`)

```481:520:cranelift/codegen/src/isa/riscv64/abi.rs
        // Adjust the stack pointer downward for clobbers, the function fixed
        // frame (spillslots and storage slots), and outgoing arguments.
        let stack_size = frame_layout.clobber_size
            + frame_layout.fixed_frame_storage_size
            + frame_layout.outgoing_args_size;

        // Store each clobbered register in order at offsets from SP,
        // placing them above the fixed frame slots.
        if stack_size > 0 {
            insts.extend(Self::gen_sp_reg_adjust(-(stack_size as i32)));

            let mut cur_offset = 0;
            for reg in &frame_layout.clobbered_callee_saves {
                let r_reg = reg.to_reg();
                let ty = match r_reg.class() {
                    RegClass::Int => I64,
                    RegClass::Float => F64,
                    RegClass::Vector => I8X16,
                };
                cur_offset = align_to(cur_offset, ty.bytes());
                insts.push(Inst::gen_store(
                    AMode::SPOffset(i64::from(stack_size - cur_offset - ty.bytes())),
                    Reg::from(reg.to_reg()),
                    ty,
                    MemFlags::trusted(),
                ));

                if flags.unwind_info() {
                    insts.push(Inst::Unwind {
                        inst: UnwindInst::SaveReg {
                            clobber_offset: frame_layout.clobber_size - cur_offset - ty.bytes(),
                            reg: r_reg,
                        },
                    });
                }

                cur_offset += ty.bytes();
                assert!(cur_offset <= stack_size);
            }
        }
```

## Memory Layout Details

### Setup Area (if allocated)

- **Offset from FP**: 0 bytes → Old FP (x8)
- **Offset from FP**: 8 bytes → Return Address (RA/x1)
- **Total size**: 16 bytes

### Clobber Area

- Stored **above** fixed frame storage
- Stored from **top downward** (highest offset first)
- Each integer/float register: 8 bytes
- Vector registers: 16 bytes (aligned to 16)
- Total size aligned to 16 bytes

### Stack Alignment

- Stack alignment: **16 bytes** (from `stack_align()`)
- All stack allocations are aligned accordingly

## Callee-Saved Registers (from DEFAULT_CALLEE_SAVES)

```730:756:cranelift/codegen/src/isa/riscv64/abi.rs
const DEFAULT_CALLEE_SAVES: PRegSet = PRegSet::empty()
    // X Regs
    .with(px_reg(2))
    .with(px_reg(8))
    .with(px_reg(9))
    .with(px_reg(18))
    .with(px_reg(19))
    .with(px_reg(20))
    .with(px_reg(21))
    .with(px_reg(22))
    .with(px_reg(23))
    .with(px_reg(24))
    .with(px_reg(25))
    .with(px_reg(26))
    .with(px_reg(27))
    // F Regs
    .with(pf_reg(8))
    .with(pf_reg(18))
    .with(pf_reg(19))
    .with(pf_reg(20))
    .with(pf_reg(21))
    .with(pf_reg(22))
    .with(pf_reg(23))
    .with(pf_reg(24))
    .with(pf_reg(25))
    .with(pf_reg(26))
    .with(pf_reg(27));
```

Note: x2 (SP) and x8 (FP) are special - FP is saved in the setup area, SP is not saved.

## Example Frame Layout

For a function that:

- Makes function calls (needs setup area)
- Clobbers 2 callee-saved registers (x9, x18)
- Has 32 bytes of fixed storage
- Has 16 bytes of stack slots
- Has 64 bytes for outgoing args

```
High Addresses
┌─────────────────────────────┐
│ Caller's Frame              │
└─────────────────────────────┘
        │
        │ FP+8  │ RA (x1)      │ ← 8 bytes
        ├───────┤──────────────┤
        │ FP+0  │ Old FP (x8)  │ ← 8 bytes
        ├───────┤──────────────┤
        │       │ x18 (saved)  │ ← 8 bytes (clobber)
        ├───────┤──────────────┤
        │       │ x9  (saved)  │ ← 8 bytes (clobber)
        ├───────┤──────────────┤
        │       │              │
        │       │ Fixed Storage│ ← 32 bytes
        │       │              │
        ├───────┤──────────────┤
        │       │              │
        │       │ Stack Slots  │ ← 16 bytes
        │       │              │
        ├───────┤──────────────┤
        │       │              │
        │       │ Outgoing Args│ ← 64 bytes
        │       │              │
        └───────┴──────────────┘
                │
                ▼ SP
```

Total frame size = 16 (setup) + 16 (clobber, aligned) + 32 + 16 + 64 = 144 bytes

## How the Frame Pointer Works

The frame pointer (FP, register x8) provides a **stable reference point** for accessing stack locations, especially when the stack pointer (SP) changes during function execution.

### Key Concept: FP vs SP

- **SP (Stack Pointer)**: Moves as the stack grows/shrinks (e.g., when calling functions, using alloca, etc.)
- **FP (Frame Pointer)**: Points to a **fixed location** in the current frame, established at function entry

### Frame Pointer Setup (Prologue)

```338:369:cranelift/codegen/src/isa/riscv64/abi.rs
        if frame_layout.setup_area_size > 0 {
            // add  sp,sp,-16    ;; alloc stack space for fp.
            // sd   ra,8(sp)     ;; save ra.
            // sd   fp,0(sp)     ;; store old fp.
            // mv   fp,sp        ;; set fp to sp.
            insts.extend(Self::gen_sp_reg_adjust(-16));
            insts.push(Inst::gen_store(
                AMode::SPOffset(8),
                link_reg(),
                I64,
                MemFlags::trusted(),
            ));
            insts.push(Inst::gen_store(
                AMode::SPOffset(0),
                fp_reg(),
                I64,
                MemFlags::trusted(),
            ));

            if flags.unwind_info() {
                insts.push(Inst::Unwind {
                    inst: UnwindInst::PushFrameRegs {
                        offset_upward_to_caller_sp: frame_layout.setup_area_size,
                    },
                });
            }
            insts.push(Inst::Mov {
                rd: writable_fp_reg(),
                rm: stack_reg(),
                ty: I64,
            });
        }
```

**Step-by-step process:**

1. **Allocate setup area**: `add sp, sp, -16` - Decrement SP by 16 bytes
2. **Save return address**: `sd ra, 8(sp)` - Store RA at SP+8
3. **Save old FP**: `sd fp, 0(sp)` - Store caller's FP at SP+0
4. **Set new FP**: `mv fp, sp` - Copy SP to FP (FP now points to setup area)

### Visual: FP Setup Sequence

```
Before prologue:
┌─────────────────┐
│ Caller's Frame  │
└─────────────────┘
        │
        ▼ SP (caller's SP)
        ▼ FP (caller's FP)

After "add sp, sp, -16":
┌─────────────────┐
│ Caller's Frame  │
└─────────────────┘
        │
        │ FP+8  │ (empty)        │ ← SP points here
        ├───────┤───────────────┤
        │ FP+0  │ (empty)        │
        └───────┴───────────────┘
        ▼ SP (moved down 16 bytes)
        ▼ FP (still caller's FP)

After saving RA and old FP, then "mv fp, sp":
┌─────────────────┐
│ Caller's Frame  │
└─────────────────┘
        │
        │ FP+8  │ RA (saved)     │ ← FP points here (fixed!)
        ├───────┤────────────────┤
        │ FP+0  │ Old FP (saved) │
        └───────┴────────────────┘
        ▼ FP (now points to setup area)
        ▼ SP (will move down further)
```

### Why FP is Useful

**1. Stable Access to Incoming Arguments**

When arguments are passed on the stack, they're placed by the **caller** at fixed offsets relative to the caller's SP. The callee needs to access them, but SP moves as the function allocates stack space.

```142:150:cranelift/codegen/src/isa/riscv64/inst/args.rs
            &AMode::IncomingArg(offset) => {
                let frame_layout = state.frame_layout();
                let sp_offset = frame_layout.tail_args_size
                    + frame_layout.setup_area_size
                    + frame_layout.clobber_size
                    + frame_layout.fixed_frame_storage_size
                    + frame_layout.outgoing_args_size;
                i64::from(sp_offset) - offset
            }
```

Incoming arguments are accessed via `IncomingArg` addressing mode, which computes the offset relative to SP. However, FP provides a more stable reference when SP changes.

**2. Frame Chain for Debugging/Unwinding**

Each frame stores the previous frame's FP at `FP+0`, creating a linked list of frames:

```
Current Frame:
  FP → [Old FP] → Points to caller's frame
       [RA]      → Return address

Caller's Frame:
  FP → [Old FP] → Points to caller's caller's frame
       [RA]      → Return address
```

This chain allows:

- **Stack unwinding**: Walk up the call stack
- **Debugging**: Inspect variables in caller frames
- **Exception handling**: Unwind to find exception handlers

**3. Consistent Offsets Despite SP Changes**

Even if SP moves (due to variable stack allocations, alloca, or dynamic stack growth), FP remains fixed, so offsets from FP stay constant.

### Frame Pointer Usage: Accessing Stack Locations

The codebase uses different addressing modes:

```124:133:cranelift/codegen/src/isa/riscv64/inst/args.rs
    pub(crate) fn get_base_register(&self) -> Option<Reg> {
        match self {
            &AMode::RegOffset(reg, ..) => Some(reg),
            &AMode::SPOffset(..) => Some(stack_reg()),
            &AMode::FPOffset(..) => Some(fp_reg()),
            &AMode::SlotOffset(..) => Some(stack_reg()),
            &AMode::IncomingArg(..) => Some(stack_reg()),
            &AMode::Const(..) | AMode::Label(..) => None,
        }
    }
```

- **`FPOffset(offset)`**: Access relative to FP (stable reference)
- **`SPOffset(offset)`**: Access relative to SP (changes as stack grows)
- **`IncomingArg(offset)`**: Access incoming arguments (computed relative to SP)

### Frame Pointer Restoration (Epilogue)

```374:405:cranelift/codegen/src/isa/riscv64/abi.rs
    fn gen_epilogue_frame_restore(
        call_conv: isa::CallConv,
        _flags: &settings::Flags,
        _isa_flags: &RiscvFlags,
        frame_layout: &FrameLayout,
    ) -> SmallInstVec<Inst> {
        let mut insts = SmallVec::new();

        if frame_layout.setup_area_size > 0 {
            insts.push(Inst::gen_load(
                writable_link_reg(),
                AMode::SPOffset(8),
                I64,
                MemFlags::trusted(),
            ));
            insts.push(Inst::gen_load(
                writable_fp_reg(),
                AMode::SPOffset(0),
                I64,
                MemFlags::trusted(),
            ));
            insts.extend(Self::gen_sp_reg_adjust(16));
        }

        if call_conv == isa::CallConv::Tail && frame_layout.tail_args_size > 0 {
            insts.extend(Self::gen_sp_reg_adjust(
                frame_layout.tail_args_size.try_into().unwrap(),
            ));
        }

        insts
    }
```

**Restoration sequence:**

1. **Restore RA**: `ld ra, 8(sp)` - Load return address
2. **Restore FP**: `ld fp, 0(sp)` - Load caller's FP (restores frame chain)
3. **Deallocate setup**: `add sp, sp, 16` - Restore SP

### Special Case: Adjusting FP When Stack Grows

If the function needs more incoming argument space after the prologue, FP must be adjusted:

```438:467:cranelift/codegen/src/isa/riscv64/abi.rs
        let incoming_args_diff = frame_layout.tail_args_size - frame_layout.incoming_args_size;
        if incoming_args_diff > 0 {
            // Decrement SP by the amount of additional incoming argument space we need
            insts.extend(Self::gen_sp_reg_adjust(-(incoming_args_diff as i32)));

            if setup_frame {
                // Write the lr position on the stack again, as it hasn't changed since it was
                // pushed in `gen_prologue_frame_setup`
                insts.push(Inst::gen_store(
                    AMode::SPOffset(8),
                    link_reg(),
                    I64,
                    MemFlags::trusted(),
                ));
                insts.push(Inst::gen_load(
                    writable_fp_reg(),
                    AMode::SPOffset(i64::from(incoming_args_diff)),
                    I64,
                    MemFlags::trusted(),
                ));
                insts.push(Inst::gen_store(
                    AMode::SPOffset(0),
                    fp_reg(),
                    I64,
                    MemFlags::trusted(),
                ));

                // Finally, sync the frame pointer with SP
                insts.push(Inst::gen_move(writable_fp_reg(), stack_reg(), I64));
            }
        }
```

This ensures FP continues to point to the correct location even when the stack grows.

### Summary: Frame Pointer Benefits

1. **Stable Reference**: FP doesn't move during function execution (unlike SP)
2. **Frame Chaining**: Enables stack unwinding and debugging
3. **Consistent Offsets**: Stack-allocated variables have fixed offsets from FP
4. **Easier Debugging**: Debuggers can walk the frame chain
5. **Exception Handling**: Unwinding can traverse frames using FP chain

The tradeoff is using one register (x8) and a small amount of stack space (16 bytes), but the benefits for debugging, unwinding, and code generation often outweigh the cost.

## Multi-Return Values (More Than 4 Return Values)

When a function returns more values than can fit in registers, the RISC-V ABI uses a **return area** on the stack. This mechanism allows functions to return an arbitrary number of values.

### Return Register Limits

```105:108:cranelift/codegen/src/isa/riscv64/abi.rs
        let (x_start, x_end, f_start, f_end) = match args_or_rets {
            ArgsOrRets::Args => (10, 17, 10, 17),
            ArgsOrRets::Rets => (10, 11, 10, 11),
        };
```

**Return value registers:**

- **Integer returns**: x10-x11 (a0-a1) - **2 registers**
- **Float returns**: f10-f11 (fa0-fa1) - **2 registers**
- **Total**: Up to **4 return values** can fit in registers

When there are more than 4 return values (or when `enable_multi_ret_implicit_sret()` is enabled), excess values are placed on the stack in a **return area**.

### Return Area Pointer

When multi-return is needed, the caller passes a **hidden argument** - a pointer to the return area:

```114:125:cranelift/codegen/src/isa/riscv64/abi.rs
        let ret_area_ptr = if add_ret_area_ptr {
            assert!(ArgsOrRets::Args == args_or_rets);
            next_x_reg += 1;
            Some(ABIArg::reg(
                x_reg(x_start).to_real_reg().unwrap(),
                I64,
                ir::ArgumentExtension::None,
                ir::ArgumentPurpose::Normal,
            ))
        } else {
            None
        };
```

**Key points:**

- The return area pointer is passed in **x10 (a0)** - the first argument register
- This consumes one argument register slot
- It's a **hidden/implicit** argument (not visible in the function signature)
- The pointer points to memory allocated by the **caller**

### How Multi-Return Works

#### 1. Caller Side (Function Call)

```
Caller's Frame:
┌─────────────────────────────┐
│                             │
│  Return Area (allocated)    │ ← Space for return values
│  - Size: sum of all return │   that don't fit in registers
│    value sizes              │
│  - Aligned to 16 bytes      │
│                             │
└─────────────────────────────┘
        │
        │ Address passed as hidden arg
        ▼
```

**Caller's responsibilities:**

1. Allocate space on the stack for return values that won't fit in registers
2. Pass the address of this area as the first argument (x10/a0)
3. After the call, read return values from this area

#### 2. Callee Side (Function Return)

When returning values:

```1694:1736:cranelift/codegen/src/machinst/abi.rs
                        &ABIArgSlot::Stack {
                            offset,
                            ty,
                            extension,
                            ..
                        } => {
                            let mut ty = ty;
                            let from_bits = ty_bits(ty) as u8;
                            // A machine ABI implementation should ensure that stack frames
                            // have "reasonable" size. All current ABIs for machinst
                            // backends (aarch64 and x64) enforce a 128MB limit.
                            let off = i32::try_from(offset).expect(
                                "Argument stack offset greater than 2GB; should hit impl limit first",
                                );
                            let ext = M::get_ext_mode(sigs[self.sig].call_conv, extension);
                            // Trash the from_reg; it should be its last use.
                            match (ext, from_bits) {
                                (ir::ArgumentExtension::Uext, n)
                                | (ir::ArgumentExtension::Sext, n)
                                    if n < word_bits =>
                                {
                                    assert_eq!(M::word_reg_class(), from_reg.class());
                                    let signed = ext == ir::ArgumentExtension::Sext;
                                    let dst =
                                        writable_value_regs(vregs.alloc_with_deferred_error(ty))
                                            .only_reg()
                                            .unwrap();
                                    ret.push(M::gen_extend(
                                        dst, from_reg, signed, from_bits,
                                        /* to_bits = */ word_bits,
                                    ));
                                    // Store the extended version.
                                    ty = M::word_type();
                                }
                                _ => {}
                            };
                            ret.push(M::gen_store_base_offset(
                                self.ret_area_ptr.unwrap(),
                                off,
                                from_reg,
                                ty,
                            ));
                        }
```

**Callee's responsibilities:**

1. Receive the return area pointer in x10 (a0)
2. Store return values that don't fit in registers to memory at offsets from this pointer
3. Return register values normally in x10-x11, f10-f11

### Return Area Setup

The callee sets up the return area pointer at function entry:

```1755:1776:cranelift/codegen/src/machinst/abi.rs
    pub fn gen_retval_area_setup(
        &mut self,
        sigs: &SigSet,
        vregs: &mut VRegAllocator<M::I>,
    ) -> Option<M::I> {
        if let Some(i) = sigs[self.sig].stack_ret_arg {
            let ret_area_ptr = Writable::from_reg(self.ret_area_ptr.unwrap());
            let insts =
                self.gen_copy_arg_to_regs(sigs, i.into(), ValueRegs::one(ret_area_ptr), vregs);
            insts.into_iter().next().map(|inst| {
                trace!(
                    "gen_retval_area_setup: inst {:?}; ptr reg is {:?}",
                    inst,
                    ret_area_ptr.to_reg()
                );
                inst
            })
        } else {
            trace!("gen_retval_area_setup: not needed");
            None
        }
    }
```

This copies the return area pointer from the argument register (x10) into a callee-saved location if needed.

### Example: Function Returning 10 Values

Consider a function that returns 10 i64 values:

**Return value allocation:**

- **Values 1-2**: x10 (a0), x11 (a1) - in registers
- **Values 3-10**: Stored in return area on stack

**Caller's frame layout:**

```
High Addresses
┌─────────────────────────────┐
│ Caller's Local Variables   │
└─────────────────────────────┘
        │
        │ Return Area (64 bytes)
        │ ┌───────────────────┐
        │ │ ret_val[9] (i64)  │ ← offset +56
        │ ├───────────────────┤
        │ │ ret_val[8] (i64)  │ ← offset +48
        │ ├───────────────────┤
        │ │ ret_val[7] (i64)  │ ← offset +40
        │ ├───────────────────┤
        │ │ ret_val[6] (i64)  │ ← offset +32
        │ ├───────────────────┤
        │ │ ret_val[5] (i64)  │ ← offset +24
        │ ├───────────────────┤
        │ │ ret_val[4] (i64)  │ ← offset +16
        │ ├───────────────────┤
        │ │ ret_val[3] (i64)  │ ← offset +8
        │ ├───────────────────┤
        │ │ ret_val[2] (i64)  │ ← offset +0 (but in x11)
        │ └───────────────────┘
        │
        ▼ SP (after allocating return area)
```

**Call sequence:**

1. Caller allocates 64 bytes (8 values × 8 bytes) for return area
2. Caller passes return area address in x10 (a0)
3. Callee receives pointer in x10
4. Callee stores ret_val[2] through ret_val[9] to return area
5. Callee returns ret_val[0] in x10, ret_val[1] in x11
6. Caller reads ret_val[0] and ret_val[1] from registers
7. Caller reads ret_val[2] through ret_val[9] from return area

### Error Handling

If multi-return is not enabled and too many return values are requested:

```157:163:cranelift/codegen/src/isa/riscv64/abi.rs
                    if args_or_rets == ArgsOrRets::Rets && !flags.enable_multi_ret_implicit_sret() {
                        return Err(crate::CodegenError::Unsupported(
                            "Too many return values to fit in registers. \
                            Use a StructReturn argument instead. (#9510)"
                                .to_owned(),
                        ));
                    }
```

The compiler will error unless:

- `enable_multi_ret_implicit_sret()` flag is set, OR
- The function uses `StructReturn` (explicit struct return mechanism)

### Return Area in Function Calls

When making function calls that return multiple values, the caller must allocate space:

```1926:1947:cranelift/codegen/src/machinst/abi.rs
        // Finally, set the stack-return pointer to the return argument area.
        // For tail calls, this means forwarding the incoming stack-return pointer.
        if let Some(ret_arg) = sigs.get_ret_arg(sig) {
            let ret_area = if is_tail_call {
                self.ret_area_ptr.expect(
                    "if the tail callee has a return pointer, then the tail caller must as well",
                )
            } else {
                let tmp = vregs.alloc_with_deferred_error(word_ty).only_reg().unwrap();
                let amode = StackAMode::OutgoingArg(stack_arg_space.into());
                insts.push(M::gen_get_stack_addr(amode, Writable::from_reg(tmp)));
                tmp
            };
            match ret_arg {
                // The return pointer must occupy a single slot.
                ABIArg::Slots { slots, .. } => {
                    assert_eq!(slots.len(), 1);
                    process_arg_slot(&mut insts, slots[0], ret_area, word_ty);
                }
                _ => unreachable!(),
            }
        }
```

The return area is typically allocated in the **outgoing arguments area** of the caller's frame.

### Summary: Multi-Return Mechanism

1. **Register limit**: Only 2 integer + 2 float = 4 return values fit in registers
2. **Return area**: Excess values are stored in memory allocated by the caller
3. **Hidden argument**: Return area pointer passed in x10 (a0)
4. **Storage**: Callee stores excess return values at offsets from the return area pointer
5. **Retrieval**: Caller reads register returns from x10-x11, f10-f11, and stack returns from the return area

This mechanism allows functions to return an arbitrary number of values while maintaining ABI compatibility and efficient register usage for the common case of small return sets.
