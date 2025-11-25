//! RISC-V 32-bit instruction encoder.
//!
//! This crate provides functions to encode RISC-V 32-bit instructions
//! into their binary representation.

#![no_std]

mod encode;
mod regs;

pub use encode::*;
pub use regs::Gpr;
