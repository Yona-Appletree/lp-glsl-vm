# ESP32-C3 JIT Test

Minimal test program that runs the JIT compiler test on real ESP32-C3 hardware.

## Differences from embive-program

- **No embive transpilation**: Directly executes compiled RISC-V code from memory
- **Real hardware**: Runs on ESP32-C3 microcontroller
- **Direct execution**: Code is loaded into RAM and executed directly (RAM is executable on ESP32-C3)

## Building

```bash
cargo build --release --target riscv32imc-unknown-none-elf
```

## Flashing

Use your preferred ESP32-C3 flashing tool (e.g., `espflash`, `cargo-espflash`):

```bash
cargo espflash flash --release --target riscv32imc-unknown-none-elf
```

## Shared Code

The JIT compilation logic is shared with `embive-program` via the `riscv-shared` crate. Both programs:

1. Build IR for a multiplication function
2. Compile IR to RISC-V code
3. Generate ELF file

The difference is in execution:

- **embive-program**: Transpiles ELF to embive bytecode and executes via VM
- **esp32c3-jit-test**: Loads RISC-V code directly into executable memory and calls it
