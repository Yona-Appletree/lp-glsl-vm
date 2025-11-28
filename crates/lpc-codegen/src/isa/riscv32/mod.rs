// Old backend (renamed, kept for reference only)
// pub mod backend_old;  // Commented out - not compiled

// New backend3 (in development)
pub mod backend3;

// Re-export modules for lib.rs
pub mod asm_parser;
pub mod decode;
pub mod disasm;
pub mod encode;
pub mod inst;
pub mod inst_buffer;
pub mod register_role;
pub mod regs;
