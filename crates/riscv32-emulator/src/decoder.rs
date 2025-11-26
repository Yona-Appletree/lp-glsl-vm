//! Instruction decoder for RISC-V 32-bit instructions.
//!
//! This module re-exports the decoder from riscv32-encoder to maintain
//! backward compatibility.

pub use riscv32_encoder::decode_instruction;
