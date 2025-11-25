//! VM runner for executing compiled functions.

extern crate alloc;

use alloc::{string::String, vec::Vec};

use core::num::NonZeroI32;
use embive::interpreter::{
    memory::{SliceMemory, RAM_OFFSET},
    Error, Interpreter, State, SYSCALL_ARGS,
};
use embive::transpiler::transpile_elf;
use r5_ir::Function;

/// Result of running a test function.
#[derive(Debug)]
pub struct TestResult {
    /// Return value from the function (if available)
    pub return_value: Option<i32>,
    /// Panic information (if panic occurred)
    pub panic_info: Option<String>,
    /// Memory state (for memory expectations)
    pub memory: Vec<u8>,
}

/// Expectations for test results.
#[derive(Debug, Clone)]
pub enum Expectation {
    /// Expect a single return value
    ReturnValue(i32),
    /// Expect multiple return values
    ReturnValues(Vec<i32>),
    /// Expect a panic with optional message
    Panic { message: Option<String> },
    /// Expect no panic
    NoPanic,
    /// Expect memory value at address
    Memory { address: u32, value: Vec<u8> },
}

impl Expectation {
    /// Check if this expectation is met.
    pub fn check(
        &self,
        result: &TestResult,
        func: &Function,
        args: &[i32],
    ) -> Result<(), String> {
        match self {
            Expectation::ReturnValue(expected) => {
                match result.return_value {
                    Some(actual) if actual == *expected => Ok(()),
                    Some(actual) => Err(format!(
                        "Expected return value {}, got {}",
                        expected, actual
                    )),
                    None => Err(format!(
                        "Expected return value {}, but function returned no value",
                        expected
                    )),
                }
            }
            Expectation::ReturnValues(expected) => {
                // For now, we only support single return value
                // TODO: Support multiple return values once calling convention is implemented
                if expected.len() == 1 {
                    Expectation::ReturnValue(expected[0]).check(result, func, args)
                } else {
                    Err(format!(
                        "Multiple return values not yet supported (expected {:?})",
                        expected
                    ))
                }
            }
            Expectation::Panic { message } => {
                match &result.panic_info {
                    Some(panic_msg) => {
                        if let Some(expected_msg) = message {
                            if panic_msg.contains(expected_msg) {
                                Ok(())
                            } else {
                                Err(format!(
                                    "Expected panic with message containing '{}', got '{}'",
                                    expected_msg, panic_msg
                                ))
                            }
                        } else {
                            Ok(())
                        }
                    }
                    None => Err(format!(
                        "Expected panic{}, but no panic occurred",
                        message
                            .as_ref()
                            .map(|m| format!(" with message containing '{}'", m))
                            .unwrap_or_default()
                    )),
                }
            }
            Expectation::NoPanic => {
                if result.panic_info.is_some() {
                    Err(format!(
                        "Expected no panic, but panic occurred: {:?}",
                        result.panic_info
                    ))
                } else {
                    Ok(())
                }
            }
            Expectation::Memory { address, value } => {
                let addr = *address as usize;
                if addr + value.len() > result.memory.len() {
                    return Err(format!(
                        "Memory address 0x{:08x} is out of bounds (memory size: {})",
                        address,
                        result.memory.len()
                    ));
                }

                let actual = &result.memory[addr..addr + value.len()];
                if actual == value.as_slice() {
                    Ok(())
                } else {
                    Err(format!(
                        "Memory mismatch at address 0x{:08x}\n  Expected: {:02x?}\n  Actual:   {:02x?}",
                        address, value, actual
                    ))
                }
            }
        }
    }
}

/// Runner for executing compiled functions in the VM.
pub struct VmRunner {
    ram_size: usize,
    max_cycles: u64,
    timeout_ms: u64,
}

impl VmRunner {
    /// Create a new VM runner with default limits.
    pub fn new(ram_size: usize) -> Self {
        Self {
            ram_size,
            max_cycles: 100_000,
            timeout_ms: 100,
        }
    }

    /// Set maximum cycle count.
    pub fn max_cycles(mut self, cycles: u64) -> Self {
        self.max_cycles = cycles;
        self
    }

    /// Set timeout in milliseconds.
    pub fn timeout_ms(mut self, ms: u64) -> Self {
        self.timeout_ms = ms;
        self
    }

    /// Run a compiled function with the given arguments.
    ///
    /// # Arguments
    ///
    /// * `elf_data` - ELF binary data
    /// * `args` - Function arguments (currently unused, function should use syscall to get args)
    ///
    /// # Returns
    ///
    /// Test result with return value, panic info, and memory state.
    pub fn run(&mut self, elf_data: &[u8], _args: &[i32]) -> Result<TestResult, String> {
        use std::sync::mpsc;
        use std::thread;
        use std::time::Duration;

        // Transpile ELF to embive bytecode
        const MAX_BINARY_SIZE: usize = 4 * 1024 * 1024;
        let mut combined = vec![0u8; MAX_BINARY_SIZE];
        let binary_size = transpile_elf(elf_data, &mut combined)
            .map_err(|e| format!("Failed to transpile ELF: {:?}", e))?;

        // Split ROM (low addresses) and RAM (high addresses)
        let code_size = RAM_OFFSET.min(binary_size as u32) as usize;
        let mut code_vec = vec![0u8; code_size.max(1)];
        if code_size > 0 {
            code_vec[..code_size].copy_from_slice(&combined[..code_size]);
        }

        // Initialize RAM
        let mut ram = vec![0u8; self.ram_size];
        if binary_size > code_size {
            let ram_offset_in_combined = code_size;
            let ram_size = (binary_size - ram_offset_in_combined).min(ram.len());
            ram[..ram_size].copy_from_slice(
                &combined[ram_offset_in_combined..ram_offset_in_combined + ram_size],
            );
        }

        // Run in a separate thread with timeout and cycle limits
        let (tx, rx) = mpsc::channel();
        let max_cycles = self.max_cycles;
        let timeout = Duration::from_millis(self.timeout_ms);

        let handle = thread::spawn(move || {
            let result = Self::run_with_limits(code_vec, ram, max_cycles);
            let _ = tx.send(result);
        });

        // Wait for result with timeout
        match rx.recv_timeout(timeout) {
            Ok(result) => {
                // Wait for thread to finish
                let _ = handle.join();
                result
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // Thread is still running, we timed out
                Err(format!(
                    "Test timed out after {}ms (possible infinite loop or hang)",
                    self.timeout_ms
                ))
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                // Thread panicked or disconnected
                let _ = handle.join();
                Err("Test thread disconnected unexpectedly".to_string())
            }
        }
    }

    /// Run interpreter with cycle and timeout limits.
    fn run_with_limits(
        mut code_vec: Vec<u8>,
        mut ram: Vec<u8>,
        max_cycles: u64,
    ) -> Result<TestResult, String> {
        // Capture pointers and lengths for syscall handler (before borrowing)
        let code_vec_ptr = code_vec.as_ptr();
        let code_vec_len = code_vec.len();
        let ram_ptr = ram.as_ptr();
        let ram_len = ram.len();

        // Create memory and interpreter
        let mut memory = SliceMemory::new(&code_vec, &mut ram);
        let mut interpreter = Interpreter::new(&mut memory, 0);
        interpreter.program_counter = 0;

        // Track results
        let mut last_result: Option<i32> = None;
        let mut panic_info: Option<String> = None;

        // Syscall handler
        let mut syscall = |nr: i32,
                           args: &[i32; SYSCALL_ARGS],
                           _memory: &mut SliceMemory|
         -> Result<Result<i32, NonZeroI32>, Error> {
            match nr {
                0 => {
                    // Syscall 0: Done - store result
                    last_result = Some(args[0]);
                    Ok(Ok(0))
                }
                1 => {
                    // Syscall 1: Panic
                    let msg_ptr = args[0] as u32;
                    let msg_len = args[1] as usize;
                    let file_ptr = args[2] as u32;
                    let file_len = args[3] as usize;
                    let line = args[4] as u32;

                    // Read panic message from memory (using unsafe like R5Vm does)
                    let msg = if msg_len > 0 {
                        let mut buf = vec![0u8; msg_len];
                        unsafe {
                            if msg_ptr < RAM_OFFSET {
                                let offset = msg_ptr as usize;
                                if offset + msg_len <= code_vec_len {
                                    core::ptr::copy_nonoverlapping(
                                        code_vec_ptr.add(offset),
                                        buf.as_mut_ptr(),
                                        msg_len,
                                    );
                                }
                            } else {
                                let offset = (msg_ptr - RAM_OFFSET) as usize;
                                if offset + msg_len <= ram_len {
                                    core::ptr::copy_nonoverlapping(
                                        ram_ptr.add(offset),
                                        buf.as_mut_ptr(),
                                        msg_len,
                                    );
                                }
                            }
                        }
                        String::from_utf8_lossy(&buf).to_string()
                    } else {
                        "panic occurred".to_string()
                    };

                    // Read file name if available
                    let file = if file_len > 0 {
                        let mut buf = vec![0u8; file_len];
                        unsafe {
                            if file_ptr < RAM_OFFSET {
                                let offset = file_ptr as usize;
                                if offset + file_len <= code_vec_len {
                                    core::ptr::copy_nonoverlapping(
                                        code_vec_ptr.add(offset),
                                        buf.as_mut_ptr(),
                                        file_len,
                                    );
                                }
                            } else {
                                let offset = (file_ptr - RAM_OFFSET) as usize;
                                if offset + file_len <= ram_len {
                                    core::ptr::copy_nonoverlapping(
                                        ram_ptr.add(offset),
                                        buf.as_mut_ptr(),
                                        file_len,
                                    );
                                }
                            }
                        }
                        Some(String::from_utf8_lossy(&buf).to_string())
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

                    Ok(Err(NonZeroI32::new(1).unwrap()))
                }
                2 => {
                    // Syscall 2: Write (ignore in tests)
                    Ok(Ok(0))
                }
                1000 => {
                    // Syscall 1000: Add two numbers
                    Ok(Ok(args[0] + args[1]))
                }
                _ => Err(Error::Custom("Unknown syscall")),
            }
        };

        // Run with cycle counting
        let mut cycles = 0u64;
        let run_result = loop {
            if cycles >= max_cycles {
                break Err(format!(
                    "Test exceeded maximum cycle count of {}",
                    max_cycles
                ));
            }

            match interpreter
                .run()
                .map_err(|e| format!("Interpreter error: {:?}", e))?
            {
                State::Running => {
                    cycles += 1;
                }
                State::Called => {
                    interpreter
                        .syscall(&mut syscall)
                        .map_err(|e| format!("Syscall error: {:?}", e))?;
                    cycles += 1;
                }
                State::Waiting => {
                    cycles += 1;
                }
                State::Halted => break Ok(()),
            }
        };

        // Extract memory state
        let memory_size = ram.len().min(4 * 1024 * 1024);
        let mut memory_snapshot = Vec::with_capacity(memory_size);

        const CHUNK_SIZE: usize = 64 * 1024;
        for offset in (0..memory_size).step_by(CHUNK_SIZE) {
            let chunk_size = (memory_size - offset).min(CHUNK_SIZE);
            // Read from RAM directly
            let end = (offset + chunk_size).min(ram.len());
            memory_snapshot.extend_from_slice(&ram[offset..end]);
        }

        if memory_snapshot.len() < memory_size {
            memory_snapshot.resize(memory_size, 0);
        }

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
}
