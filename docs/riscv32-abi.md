# RISC-V 32-bit Call Frame Layout

Based on the RISC-V User-Level ISA specification (Chapter 18: Calling Convention) and aligned with RISC-V 64-bit ABI patterns.

## Register Usage

- **SP (Stack Pointer)**: x2 (`stack_reg()`)
- **FP (Frame Pointer)**: x8 (`fp_reg()` / `s0`)
- **RA/LR (Return Address)**: x1 (`link_reg()`)

## C Datatypes and Alignment (RV32)

Table 18.1 from the RISC-V specification:

| C type | Description | Bytes in RV32 |
|--------|-------------|---------------|
| `char` | Character value/byte | 1 |
| `short` | Short integer | 2 |
| `int` | Integer | 4 |
| `long` | Long integer | 4 |
| `long long` | Long long integer | 8 |
| `void*` | Pointer | 4 |
| `float` | Single-precision float | 4 |
| `double` | Double-precision float | 8 |
| `long double` | Extended-precision float | 16 |

**Key differences from RV64:**
- `long` and pointers are **4 bytes** (not 8 bytes)
- RV32 uses **ILP32** integer model (int/long/pointer = 32 bits)
- All datatypes are naturally aligned when stored in memory

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
│ FP+4  │ Return Address (RA/x1)         │ ← Saved by callee
├───────┼─────────────────────────────────┤
│ FP+0  │ Old Frame Pointer (FP/x8)      │ ← Saved by callee
├───────┼─────────────────────────────────┤
│       │                                 │
│       │  Setup Area (8 bytes)           │ ← frame_layout.setup_area_size
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
│       │  - Each register: 4 bytes       │
│       │  - Double-precision floats:     │
│       │    8 bytes (f8-f9, f18-f27)    │
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

## Frame Layout Structure

From `compute_frame_layout()`:

- **setup_area_size**: 8 bytes (FP + RA) or 0
- **clobber_size**: Space for callee-saved registers that are clobbered
- **fixed_frame_storage_size**: Fixed storage slots
- **stackslots_size**: Spill slots for register spills
- **outgoing_args_size**: Space for outgoing arguments

## Prologue Sequence

For RV32, the prologue sequence is similar to RV64 but uses 32-bit operations:

```rust
if frame_layout.setup_area_size > 0 {
    // addi sp,sp,-8     ;; alloc stack space for fp.
    // sw  ra,4(sp)      ;; save ra.
    // sw  fp,0(sp)      ;; store old fp.
    // mv  fp,sp         ;; set fp to sp.
    insts.extend(Self::gen_sp_reg_adjust(-8));
    insts.push(Inst::gen_store(
        AMode::SPOffset(4),
        link_reg(),
        I32,
        MemFlags::trusted(),
    ));
    insts.push(Inst::gen_store(
        AMode::SPOffset(0),
        fp_reg(),
        I32,
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
        ty: I32,
    });
}
```

**Step-by-step process:**

1. **Allocate setup area**: `addi sp, sp, -8` - Decrement SP by 8 bytes
2. **Save return address**: `sw ra, 4(sp)` - Store RA at SP+4
3. **Save old FP**: `sw fp, 0(sp)` - Store caller's FP at SP+0
4. **Set new FP**: `mv fp, sp` - Copy SP to FP (FP now points to setup area)

## Clobber Save Sequence

```rust
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
            RegClass::Int => I32,
            RegClass::Float => F32,  // Single-precision
            RegClass::Vector => I8X16,
        };
        cur_offset = align_to(cur_offset, ty.bytes());
        insts.push(Inst::gen_store(
            AMode::SPOffset(i32::from(stack_size - cur_offset - ty.bytes())),
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

**Note**: For double-precision floating-point registers (f8-f9, f18-f27), each register is 8 bytes and must be aligned to 8 bytes.

## Memory Layout Details

### Setup Area (if allocated)

- **Offset from FP**: 0 bytes → Old FP (x8)
- **Offset from FP**: 4 bytes → Return Address (RA/x1)
- **Total size**: 8 bytes

### Clobber Area

- Stored **above** fixed frame storage
- Stored from **top downward** (highest offset first)
- Each integer register: **4 bytes**
- Single-precision float registers: **4 bytes**
- Double-precision float registers: **8 bytes** (aligned to 8)
- Vector registers: 16 bytes (aligned to 16)
- Total size aligned to 16 bytes

### Stack Alignment

- Stack alignment: **16 bytes** (from RISC-V ABI specification)
- All stack allocations are aligned accordingly

## Register Usage (RISC-V Calling Convention)

### Integer Registers

| Register | ABI Name | Description | Saver |
|----------|----------|-------------|-------|
| x0 | zero | Hard-wired zero | — |
| x1 | ra | Return address | Caller |
| x2 | sp | Stack pointer | Callee |
| x3 | gp | Global pointer | — |
| x4 | tp | Thread pointer | — |
| x5-7 | t0-2 | Temporaries | Caller |
| x8 | s0/fp | Saved register/frame pointer | Callee |
| x9 | s1 | Saved register | Callee |
| x10-11 | a0-1 | Function arguments/return values | Caller |
| x12-17 | a2-7 | Function arguments | Caller |
| x18-27 | s2-11 | Saved registers | Callee |
| x28-31 | t3-6 | Temporaries | Caller |

### Floating-Point Registers

| Register | ABI Name | Description | Saver |
|----------|----------|-------------|-------|
| f0-7 | ft0-7 | FP temporaries | Caller |
| f8-9 | fs0-1 | FP saved registers | Callee |
| f10-11 | fa0-1 | FP arguments/return values | Caller |
| f12-17 | fa2-7 | FP arguments | Caller |
| f18-27 | fs2-11 | FP saved registers | Callee |
| f28-31 | ft8-11 | FP temporaries | Caller |

### Callee-Saved Registers

**Integer callee-saved:**
- x8 (s0/fp), x9 (s1), x18-x27 (s2-s11)

**Floating-point callee-saved:**
- f8-f9 (fs0-fs1), f18-f27 (fs2-fs11)

**Note**: x2 (SP) and x8 (FP) are special - FP is saved in the setup area, SP is not saved.

## Argument Passing

### Integer Arguments

- **First 8 arguments**: Passed in registers `a0-a7` (x10-x17)
- **Additional arguments**: Passed on the stack
- **Stack pointer**: Points to the first argument not passed in a register

### Floating-Point Arguments

- **First 8 FP arguments**: Passed in registers `fa0-fa7` (f10-f17)
- **Additional FP arguments**: Passed on the stack
- **Special cases**:
  - FP arguments in unions or array fields of structures → passed in integer registers
  - FP arguments to variadic functions (except explicitly named) → passed in integer registers

### Argument Alignment

- Arguments smaller than a pointer-word (4 bytes) are passed in the least-significant bits of argument registers
- Sub-pointer-word arguments on the stack appear in lower addresses (little-endian)
- Primitive arguments twice the size of a pointer-word (8 bytes, e.g., `long long`, `double`) are:
  - **In registers**: Naturally aligned even-odd register pair (e.g., a2-a3)
  - **On stack**: Naturally aligned to 8 bytes
- Arguments more than twice the size of a pointer-word are passed by reference

### Example: Argument Passing

**Function**: `void foo(int, long long, double)`

- **Argument 1** (`int`): `a0` (x10)
- **Argument 2** (`long long`): `a2-a3` (x12-x13) - aligned pair, `a1` skipped
- **Argument 3** (`double`): `fa0` (f10) - if RVG convention, or `a4-a5` if soft-float

## Return Values

### Return Register Limits

- **Integer returns**: `a0-a1` (x10-x11) - **2 registers**
- **Float returns**: `fa0-fa1` (f10-f11) - **2 registers**
- **Total**: Up to **4 return values** can fit in registers

### Return Value Rules

- Floating-point values are returned in floating-point registers only if they are:
  - Primitives (single `float` or `double`)
  - Members of a struct consisting of only one or two floating-point values
- Other return values that fit into two pointer-words (8 bytes total) are returned in `a0-a1`
- Larger return values are passed entirely in memory; the caller allocates this memory region and passes a pointer to it as an implicit first parameter to the callee

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
        │ FP+4  │ RA (x1)      │ ← 4 bytes
        ├───────┤──────────────┤
        │ FP+0  │ Old FP (x8)  │ ← 4 bytes
        ├───────┤──────────────┤
        │       │ x18 (saved)  │ ← 4 bytes (clobber)
        ├───────┤──────────────┤
        │       │ x9  (saved)  │ ← 4 bytes (clobber)
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

Total frame size = 8 (setup) + 8 (clobber, aligned) + 32 + 16 + 64 = 128 bytes

## How the Frame Pointer Works

The frame pointer (FP, register x8) provides a **stable reference point** for accessing stack locations, especially when the stack pointer (SP) changes during function execution.

### Key Concept: FP vs SP

- **SP (Stack Pointer)**: Moves as the stack grows/shrinks (e.g., when calling functions, using alloca, etc.)
- **FP (Frame Pointer)**: Points to a **fixed location** in the current frame, established at function entry

### Frame Pointer Setup (Prologue)

**Step-by-step process:**

1. **Allocate setup area**: `addi sp, sp, -8` - Decrement SP by 8 bytes
2. **Save return address**: `sw ra, 4(sp)` - Store RA at SP+4
3. **Save old FP**: `sw fp, 0(sp)` - Store caller's FP at SP+0
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

After "addi sp, sp, -8":
┌─────────────────┐
│ Caller's Frame  │
└─────────────────┘
        │
        │ FP+4  │ (empty)        │ ← SP points here
        ├───────┤───────────────┤
        │ FP+0  │ (empty)        │
        └───────┴───────────────┘
        ▼ SP (moved down 8 bytes)
        ▼ FP (still caller's FP)

After saving RA and old FP, then "mv fp, sp":
┌─────────────────┐
│ Caller's Frame  │
└─────────────────┘
        │
        │ FP+4  │ RA (saved)     │ ← FP points here (fixed!)
        ├───────┤────────────────┤
        │ FP+0  │ Old FP (saved) │
        └───────┴────────────────┘
        ▼ FP (now points to setup area)
        ▼ SP (will move down further)
```

### Why FP is Useful

**1. Stable Access to Incoming Arguments**

When arguments are passed on the stack, they're placed by the **caller** at fixed offsets relative to the caller's SP. The callee needs to access them, but SP moves as the function allocates stack space.

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

Addressing modes:
- **`FPOffset(offset)`**: Access relative to FP (stable reference)
- **`SPOffset(offset)`**: Access relative to SP (changes as stack grows)
- **`IncomingArg(offset)`**: Access incoming arguments (computed relative to SP)

### Frame Pointer Restoration (Epilogue)

**Restoration sequence:**

1. **Restore RA**: `lw ra, 4(sp)` - Load return address
2. **Restore FP**: `lw fp, 0(sp)` - Load caller's FP (restores frame chain)
3. **Deallocate setup**: `addi sp, sp, 8` - Restore SP

### Special Case: Adjusting FP When Stack Grows

If the function needs more incoming argument space after the prologue, FP must be adjusted to continue pointing to the correct location even when the stack grows.

### Summary: Frame Pointer Benefits

1. **Stable Reference**: FP doesn't move during function execution (unlike SP)
2. **Frame Chaining**: Enables stack unwinding and debugging
3. **Consistent Offsets**: Stack-allocated variables have fixed offsets from FP
4. **Easier Debugging**: Debuggers can walk the frame chain
5. **Exception Handling**: Unwinding can traverse frames using FP chain

The tradeoff is using one register (x8) and a small amount of stack space (8 bytes), but the benefits for debugging, unwinding, and code generation often outweigh the cost.

## Multi-Return Values (More Than 4 Return Values)

When a function returns more values than can fit in registers, the RISC-V ABI uses a **return area** on the stack. This mechanism allows functions to return an arbitrary number of values.

### Return Register Limits

**Return value registers:**
- **Integer returns**: `a0-a1` (x10-x11) - **2 registers**
- **Float returns**: `fa0-fa1` (f10-f11) - **2 registers**
- **Total**: Up to **4 return values** can fit in registers

When there are more than 4 return values (or when `enable_multi_ret_implicit_sret()` is enabled), excess values are placed on the stack in a **return area**.

### Return Area Pointer

When multi-return is needed, the caller passes a **hidden argument** - a pointer to the return area:

**Key points:**
- The return area pointer is passed in **x10 (a0)** - the first argument register
- This consumes one argument register slot
- It's a **hidden/implicit** argument (not visible in the function signature)
- The pointer points to memory allocated by the **caller**

### How Multi-Return Works

#### 1. Caller Side (Function Call)

**Caller's responsibilities:**

1. Allocate space on the stack for return values that won't fit in registers
2. Pass the address of this area as the first argument (x10/a0)
3. After the call, read return values from this area

#### 2. Callee Side (Function Return)

**Callee's responsibilities:**

1. Receive the return area pointer in x10 (a0)
2. Store return values that don't fit in registers to memory at offsets from this pointer
3. Return register values normally in x10-x11, f10-f11

### Example: Function Returning 10 Values

Consider a function that returns 10 i32 values:

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
        │ Return Area (32 bytes)
        │ ┌───────────────────┐
        │ │ ret_val[9] (i32)  │ ← offset +28
        │ ├───────────────────┤
        │ │ ret_val[8] (i32)  │ ← offset +24
        │ ├───────────────────┤
        │ │ ret_val[7] (i32)  │ ← offset +20
        │ ├───────────────────┤
        │ │ ret_val[6] (i32)  │ ← offset +16
        │ ├───────────────────┤
        │ │ ret_val[5] (i32)  │ ← offset +12
        │ ├───────────────────┤
        │ │ ret_val[4] (i32)  │ ← offset +8
        │ ├───────────────────┤
        │ │ ret_val[3] (i32)  │ ← offset +4
        │ ├───────────────────┤
        │ │ ret_val[2] (i32)  │ ← offset +0 (but in x11)
        │ └───────────────────┘
        │
        ▼ SP (after allocating return area)
```

**Call sequence:**

1. Caller allocates 32 bytes (8 values × 4 bytes) for return area
2. Caller passes return area address in x10 (a0)
3. Callee receives pointer in x10
4. Callee stores ret_val[2] through ret_val[9] to return area
5. Callee returns ret_val[0] in x10, ret_val[1] in x11
6. Caller reads ret_val[0] and ret_val[1] from registers
7. Caller reads ret_val[2] through ret_val[9] from return area

### Error Handling

If multi-return is not enabled and too many return values are requested, the compiler will error unless:

- `enable_multi_ret_implicit_sret()` flag is set, OR
- The function uses `StructReturn` (explicit struct return mechanism)

### Return Area in Function Calls

When making function calls that return multiple values, the caller must allocate space. The return area is typically allocated in the **outgoing arguments area** of the caller's frame.

### Summary: Multi-Return Mechanism

1. **Register limit**: Only 2 integer + 2 float = 4 return values fit in registers
2. **Return area**: Excess values are stored in memory allocated by the caller
3. **Hidden argument**: Return area pointer passed in x10 (a0)
4. **Storage**: Callee stores excess return values at offsets from the return area pointer
5. **Retrieval**: Caller reads register returns from x10-x11, f10-f11, and stack returns from the return area

This mechanism allows functions to return an arbitrary number of values while maintaining ABI compatibility and efficient register usage for the common case of small return sets.

## Soft-Float Calling Convention

The soft-float calling convention is used on RV32 implementations that lack floating-point hardware. It avoids all use of instructions in the F, D, and Q standard extensions, and hence the f registers.

**Key differences from RVG convention:**

- Integral arguments are passed and returned in the same manner as the RVG convention
- Stack discipline is the same
- Floating-point arguments are passed and returned in integer registers, using the rules for integer arguments of the same size

**Example**: `double foo(int, double, long double)`

- **Argument 1** (`int`): `a0` (x10)
- **Argument 2** (`double`): `a2-a3` (x12-x13) - 8 bytes in integer registers
- **Argument 3** (`long double`): Passed by reference via `a4` (x14)
- **Result** (`double`): Returned in `a0-a1` (x10-x11) - 8 bytes in integer registers

The dynamic rounding mode and accrued exception flags are accessed through the routines provided by the C99 header `fenv.h`.

