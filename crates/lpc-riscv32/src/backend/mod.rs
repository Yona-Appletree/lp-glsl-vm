//! RISC-V 32-bit backend for compiling IR to machine code.

pub mod frame;
pub mod abi;
pub mod lower;

// Re-export for convenience
pub use frame::FrameLayout;
pub use abi::Abi;
pub use lower::Lowerer;
