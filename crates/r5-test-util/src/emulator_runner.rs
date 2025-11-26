//! Emulator runner for executing compiled functions using riscv32-emulator.

extern crate alloc;

use alloc::{string::String, vec::Vec};

use riscv32_emulator::{LogLevel, Riscv32Emulator, StepResult};

use crate::vm_runner::TestResult;

/// Runner for executing compiled functions using the RISC-V 32 emulator.
pub struct EmulatorRunner {
    max_instructions: u64,
    log_level: LogLevel,
    ram_size: usize,
}

impl EmulatorRunner {
    /// Create a new emulator runner with default limits.
    pub fn new(ram_size: usize) -> Self {
        Self {
            max_instructions: 100_000,
            log_level: LogLevel::None,
            ram_size,
        }
    }

    /// Set maximum instruction count.
    pub fn max_instructions(mut self, limit: u64) -> Self {
        self.max_instructions = limit;
        self
    }

    /// Set logging level.
    pub fn log_level(mut self, level: LogLevel) -> Self {
        self.log_level = level;
        self
    }

    /// Run compiled code and return test result.
    ///
    /// # Arguments
    ///
    /// * `code` - Compiled RISC-V code (instructions as bytes)
    /// * `_args` - Function arguments (currently unused, function should use syscall to get args)
    ///
    /// # Returns
    ///
    /// Test result with return value, panic info, and memory state.
    pub fn run(&mut self, code: &[u8], _args: &[i32]) -> Result<TestResult, String> {
        // Split code and RAM (similar to VmRunner)
        // Code goes in low addresses, RAM starts at 0x80000000
        const RAM_OFFSET: u32 = 0x80000000;
        let code_size = RAM_OFFSET.min(code.len() as u32) as usize;
        let code_vec = if code_size > 0 {
            code[..code_size].to_vec()
        } else {
            Vec::new()
        };

        // Initialize RAM
        let mut ram = vec![0u8; self.ram_size];
        if code.len() > code_size {
            let ram_size = (code.len() - code_size).min(ram.len());
            ram[..ram_size].copy_from_slice(&code[code_size..code_size + ram_size]);
        }

        // Create emulator
        let mut emu = Riscv32Emulator::new(code_vec, ram)
            .with_max_instructions(self.max_instructions)
            .with_log_level(self.log_level);

        // Track results
        let mut last_result: Option<i32> = None;
        let mut panic_info: Option<String> = None;

        // Run until halt or error
        let run_result = loop {
            match emu.step() {
                Ok(StepResult::Halted) => {
                    // EBREAK encountered, return value in a0
                    last_result = Some(emu.get_register(riscv32_encoder::Gpr::A0));
                    break Ok(());
                }
                Ok(StepResult::Syscall(syscall_info)) => {
                    // Handle syscall
                    match syscall_info.number {
                        0 => {
                            // Syscall 0: Done - store result and halt
                            last_result = Some(syscall_info.args[0]);
                            // Set a0 and execute EBREAK to halt
                            emu.set_register(riscv32_encoder::Gpr::A0, syscall_info.args[0]);
                            // We need to inject an EBREAK, but we can't modify code
                            // Instead, just break and return
                            break Ok(());
                        }
                        1 => {
                            // Syscall 1: Panic
                            let msg_ptr = syscall_info.args[0] as u32;
                            let msg_len = syscall_info.args[1] as usize;
                            let file_ptr = syscall_info.args[2] as u32;
                            let file_len = syscall_info.args[3] as usize;
                            let line = syscall_info.args[4] as u32;

                            // Read panic message from memory
                            let msg = if msg_len > 0 {
                                self.read_string_from_memory(&emu, msg_ptr, msg_len)
                            } else {
                                "panic occurred".to_string()
                            };

                            // Read file name if available
                            let file = if file_len > 0 {
                                Some(self.read_string_from_memory(&emu, file_ptr, file_len))
                            } else {
                                None
                            };

                            panic_info = Some(format!(
                                "{} at {}:{}",
                                msg,
                                file.as_deref().unwrap_or("unknown"),
                                if line > 0 {
                                    line.to_string()
                                } else {
                                    "?".to_string()
                                }
                            ));

                            // Halt on panic
                            break Ok(());
                        }
                        2 => {
                            // Syscall 2: Write (ignore in tests)
                            // Continue execution
                        }
                        1000 => {
                            // Syscall 1000: Add two numbers
                            let result = syscall_info.args[0] + syscall_info.args[1];
                            emu.set_register(riscv32_encoder::Gpr::A0, result);
                            // Continue execution
                        }
                        _ => {
                            break Err(format!("Unknown syscall: {}", syscall_info.number));
                        }
                    }
                }
                Ok(StepResult::Continue) => {
                    // Continue execution
                }
                Err(e) => {
                    break Err(format!("Emulator error: {}", e));
                }
            }
        };

        // Extract memory state
        let memory_snapshot = emu.memory().ram().to_vec();

        // Check if run failed
        if let Err(e) = run_result {
            if panic_info.is_none() {
                return Err(format!("VM run failed: {}", e));
            }
        }

        Ok(TestResult {
            return_value: last_result,
            panic_info,
            memory: memory_snapshot,
        })
    }

    /// Read a string from memory at the given address.
    fn read_string_from_memory(&self, emu: &Riscv32Emulator, ptr: u32, len: usize) -> String {
        const RAM_OFFSET: u32 = 0x80000000;
        let mut buf = vec![0u8; len];

        // Try to read from memory
        // For simplicity, we'll read from RAM region
        if ptr >= RAM_OFFSET {
            let offset = (ptr - RAM_OFFSET) as usize;
            let ram = emu.memory().ram();
            if offset + len <= ram.len() {
                buf.copy_from_slice(&ram[offset..offset + len]);
            }
        } else {
            // Code region - not typically used for strings, but handle it
            // This is a simplified implementation
        }

        String::from_utf8_lossy(&buf).to_string()
    }
}
