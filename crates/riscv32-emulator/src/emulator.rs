//! Core RISC-V 32-bit emulator implementation.

extern crate alloc;

use alloc::{format, string::String, vec::Vec};

use riscv32_encoder::Gpr;

use crate::{
    decoder::decode_instruction,
    error::EmulatorError,
    executor::execute_instruction,
    logging::{InstLog, LogLevel},
    memory::Memory,
};

/// Result of a single step.
#[derive(Debug, Clone)]
pub enum StepResult {
    /// Normal step completed, continue execution
    Continue,
    /// ECALL encountered, syscall information available
    Syscall(SyscallInfo),
    /// EBREAK encountered, execution halted
    Halted,
}

/// Information about a syscall (ECALL).
#[derive(Debug, Clone)]
pub struct SyscallInfo {
    /// Syscall number (from a7 register)
    pub number: i32,
    /// Syscall arguments (from a0-a6 registers)
    pub args: [i32; 7],
}

/// RISC-V 32-bit emulator.
pub struct Riscv32Emulator {
    regs: [i32; 32],
    pc: u32,
    memory: Memory,
    instruction_count: u64,
    max_instructions: u64,
    log_level: LogLevel,
    log_buffer: Vec<InstLog>,
}

impl Riscv32Emulator {
    /// Create a new emulator with the given code and RAM.
    ///
    /// # Arguments
    ///
    /// * `code` - Code region (instructions)
    /// * `ram` - RAM region (data)
    pub fn new(code: Vec<u8>, ram: Vec<u8>) -> Self {
        Self {
            regs: [0; 32],
            pc: 0,
            memory: Memory::with_default_addresses(code, ram),
            instruction_count: 0,
            max_instructions: 100_000,
            log_level: LogLevel::None,
            log_buffer: Vec::new(),
        }
    }

    /// Set the maximum number of instructions to execute.
    pub fn with_max_instructions(mut self, limit: u64) -> Self {
        self.max_instructions = limit;
        self
    }

    /// Set the logging level.
    pub fn with_log_level(mut self, level: LogLevel) -> Self {
        self.log_level = level;
        self
    }

    /// Execute a single instruction.
    pub fn step(&mut self) -> Result<StepResult, EmulatorError> {
        // Check instruction limit
        if self.instruction_count >= self.max_instructions {
            return Err(EmulatorError::InstructionLimitExceeded {
                limit: self.max_instructions,
                executed: self.instruction_count,
                pc: self.pc,
                regs: self.regs,
            });
        }

        // Fetch instruction
        let inst_word = self.memory.fetch_instruction(self.pc).map_err(|mut e| {
            match &mut e {
                EmulatorError::InvalidMemoryAccess {
                    regs: err_regs,
                    pc: err_pc,
                    ..
                } => {
                    *err_regs = self.regs;
                    *err_pc = self.pc;
                }
                EmulatorError::UnalignedAccess {
                    regs: err_regs,
                    pc: err_pc,
                    ..
                } => {
                    *err_regs = self.regs;
                    *err_pc = self.pc;
                }
                _ => {}
            }
            e
        })?;

        // Decode instruction
        let decoded =
            decode_instruction(inst_word).map_err(|reason| EmulatorError::InvalidInstruction {
                pc: self.pc,
                instruction: inst_word,
                reason,
                regs: self.regs,
            })?;

        // Increment instruction count before execution (for cycle counting)
        self.instruction_count += 1;

        // Execute instruction
        let exec_result = execute_instruction(decoded, self.pc, &mut self.regs, &mut self.memory)?;

        // Update PC
        self.pc = exec_result.new_pc.unwrap_or(self.pc.wrapping_add(4));

        // Log instruction with cycle count
        let log_with_cycle = exec_result.log.set_cycle(self.instruction_count);
        self.log_instruction(log_with_cycle);

        // Handle special cases
        if exec_result.should_halt {
            Ok(StepResult::Halted)
        } else if exec_result.syscall {
            // Extract syscall info from registers
            let syscall_info = SyscallInfo {
                number: self.regs[Gpr::A7.num() as usize],
                args: [
                    self.regs[Gpr::A0.num() as usize],
                    self.regs[Gpr::A1.num() as usize],
                    self.regs[Gpr::A2.num() as usize],
                    self.regs[Gpr::A3.num() as usize],
                    self.regs[Gpr::A4.num() as usize],
                    self.regs[Gpr::A5.num() as usize],
                    self.regs[Gpr::A6.num() as usize],
                ],
            };
            Ok(StepResult::Syscall(syscall_info))
        } else {
            Ok(StepResult::Continue)
        }
    }

    /// Run until EBREAK is encountered, returning the value in a0.
    pub fn run_until_ebreak(&mut self) -> Result<i32, EmulatorError> {
        loop {
            match self.step()? {
                StepResult::Halted => {
                    return Ok(self.regs[Gpr::A0.num() as usize]);
                }
                StepResult::Continue => {
                    // Continue execution
                }
                StepResult::Syscall(_) => {
                    // Treat syscall as error in this context (caller should use run_until_ecall)
                    return Err(EmulatorError::InvalidInstruction {
                        pc: self.pc,
                        instruction: 0,
                        reason: String::from("Unexpected ECALL in run_until_ebreak"),
                        regs: self.regs,
                    });
                }
            }
        }
    }

    /// Run until ECALL is encountered, returning syscall information.
    pub fn run_until_ecall(&mut self) -> Result<SyscallInfo, EmulatorError> {
        loop {
            match self.step()? {
                StepResult::Syscall(info) => {
                    return Ok(info);
                }
                StepResult::Halted => {
                    return Err(EmulatorError::InvalidInstruction {
                        pc: self.pc,
                        instruction: 0,
                        reason: String::from("Unexpected EBREAK in run_until_ecall"),
                        regs: self.regs,
                    });
                }
                StepResult::Continue => {
                    // Continue execution
                }
            }
        }
    }

    /// Get the value of a register.
    pub fn get_register(&self, reg: Gpr) -> i32 {
        if reg.num() == 0 {
            0
        } else {
            self.regs[reg.num() as usize]
        }
    }

    /// Set the value of a register.
    ///
    /// Note: Writing to x0 (ZERO) is a no-op.
    pub fn set_register(&mut self, reg: Gpr, value: i32) {
        if reg.num() != 0 {
            self.regs[reg.num() as usize] = value;
        }
    }

    /// Get the current program counter.
    pub fn get_pc(&self) -> u32 {
        self.pc
    }

    /// Set the program counter.
    pub fn set_pc(&mut self, pc: u32) {
        self.pc = pc;
    }

    /// Get the number of instructions executed so far.
    pub fn get_instruction_count(&self) -> u64 {
        self.instruction_count
    }

    /// Get captured log entries.
    pub fn get_logs(&self) -> &[InstLog] {
        &self.log_buffer
    }

    /// Format all captured logs as a string.
    pub fn format_logs(&self) -> String {
        let mut result = String::new();
        for log in &self.log_buffer {
            result.push_str(&format!("{}\n", log));
        }
        result
    }

    /// Clear captured log messages.
    pub fn clear_logs(&mut self) {
        self.log_buffer.clear();
    }

    /// Dump the current emulator state as a human-readable string.
    pub fn dump_state(&self) -> String {
        let mut result = String::new();
        result.push_str(&format!("PC: 0x{:08x}\n", self.pc));
        result.push_str(&format!(
            "Instructions executed: {}\n",
            self.instruction_count
        ));
        result.push_str("\nRegisters:\n");

        // Named registers first
        let named_regs = [
            (Gpr::ZERO, "zero"),
            (Gpr::RA, "ra"),
            (Gpr::SP, "sp"),
            (Gpr::GP, "gp"),
            (Gpr::TP, "tp"),
            (Gpr::T0, "t0"),
            (Gpr::T1, "t1"),
            (Gpr::T2, "t2"),
            (Gpr::S0, "s0"),
            (Gpr::S1, "s1"),
            (Gpr::A0, "a0"),
            (Gpr::A1, "a1"),
            (Gpr::A2, "a2"),
            (Gpr::A3, "a3"),
            (Gpr::A4, "a4"),
            (Gpr::A5, "a5"),
            (Gpr::A6, "a6"),
            (Gpr::A7, "a7"),
        ];

        for (reg, name) in &named_regs {
            let value = self.get_register(*reg);
            if value != 0 || *reg == Gpr::ZERO {
                result.push_str(&format!(
                    "  {} (x{}) = 0x{:08x} ({})\n",
                    name,
                    reg.num(),
                    value as u32,
                    value
                ));
            }
        }

        // Other registers
        for i in 18..32 {
            let reg = Gpr::new(i);
            let value = self.get_register(reg);
            if value != 0 {
                result.push_str(&format!("  x{} = 0x{:08x} ({})\n", i, value as u32, value));
            }
        }

        result
    }

    /// Log an instruction based on the current log level.
    fn log_instruction(&mut self, log: InstLog) {
        match self.log_level {
            LogLevel::None => {}
            LogLevel::Errors => {
                // Only log on errors (handled elsewhere)
            }
            LogLevel::Instructions | LogLevel::Verbose => {
                // Implement rolling buffer: if buffer reaches 100, remove oldest
                if self.log_buffer.len() >= 100 {
                    self.log_buffer.remove(0);
                }
                self.log_buffer.push(log);
            }
        }
    }

    /// Get a reference to the memory (for inspection).
    pub fn memory(&self) -> &Memory {
        &self.memory
    }

    /// Get a mutable reference to the memory (for initialization).
    pub fn memory_mut(&mut self) -> &mut Memory {
        &mut self.memory
    }

    /// Format debug information including disassembly and execution logs.
    ///
    /// # Arguments
    ///
    /// * `highlight_pc` - Optional PC to highlight in disassembly (for errors)
    /// * `log_count` - Number of recent logs to show (default 20)
    pub fn format_debug_info(&self, highlight_pc: Option<u32>, log_count: usize) -> String {
        use alloc::format;

        use riscv32_encoder::disassemble_instruction;

        let mut result = String::new();
        let code = self.memory.code();

        // Disassemble all instructions
        let mut instructions = Vec::new();
        for i in (0..code.len()).step_by(4) {
            if i + 4 <= code.len() {
                let inst_word =
                    u32::from_le_bytes([code[i], code[i + 1], code[i + 2], code[i + 3]]);
                let pc = i as u32;
                let disasm = disassemble_instruction(inst_word);
                instructions.push((pc, inst_word, disasm));
            }
        }

        // Show disassembly
        result.push_str("Disassembly:\n");
        if let Some(error_pc) = highlight_pc {
            if instructions.len() > 50 {
                // Find the index of the highlighted instruction
                let fail_idx = instructions
                    .iter()
                    .position(|(pc, _, _)| *pc == error_pc)
                    .unwrap_or(0);
                let start = fail_idx.saturating_sub(10);
                let end = (fail_idx + 11).min(instructions.len());

                if start > 0 {
                    result.push_str("  ...\n");
                }

                for (idx, (pc, _inst_word, disasm)) in instructions[start..end].iter().enumerate() {
                    let actual_idx = start + idx;
                    let marker = if *pc == error_pc { ">>> " } else { "    " };
                    result.push_str(&format!(
                        "{}{:3}: 0x{:08x}: {}\n",
                        marker, actual_idx, pc, disasm
                    ));
                }

                if end < instructions.len() {
                    result.push_str("  ...\n");
                }
            } else {
                // Show all instructions
                for (idx, (pc, _inst_word, disasm)) in instructions.iter().enumerate() {
                    let marker = if *pc == error_pc { ">>> " } else { "    " };
                    result.push_str(&format!("{}{:3}: 0x{:08x}: {}\n", marker, idx, pc, disasm));
                }
            }
        } else {
            // No highlight - show recent instructions or all if small
            if instructions.len() > 50 {
                let start = instructions.len().saturating_sub(20);
                if start > 0 {
                    result.push_str("  ...\n");
                }
                for (idx, (pc, _inst_word, disasm)) in instructions[start..].iter().enumerate() {
                    let actual_idx = start + idx;
                    result.push_str(&format!("   {:3}: 0x{:08x}: {}\n", actual_idx, pc, disasm));
                }
            } else {
                // Show all instructions
                for (idx, (pc, _inst_word, disasm)) in instructions.iter().enumerate() {
                    result.push_str(&format!("   {:3}: 0x{:08x}: {}\n", idx, pc, disasm));
                }
            }
        }

        // Show logs
        if !self.log_buffer.is_empty() {
            result.push_str("\nLast execution logs:\n");
            let start = self.log_buffer.len().saturating_sub(log_count);
            for log in &self.log_buffer[start..] {
                result.push_str(&format!("{}\n", log));
            }
        }

        result
    }
}
