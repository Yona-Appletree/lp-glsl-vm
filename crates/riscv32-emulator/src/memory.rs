//! Memory model for the RISC-V 32 emulator.

use alloc::vec::Vec;
use crate::error::{EmulatorError, MemoryAccessKind};

/// Default RAM start address (0x80000000, matching embive's RAM_OFFSET).
pub const DEFAULT_RAM_START: u32 = 0x80000000;

/// Memory model with separate code and RAM regions.
pub struct Memory {
    code: Vec<u8>,
    ram: Vec<u8>,
    code_start: u32,
    ram_start: u32,
}

impl Memory {
    /// Create a new memory model.
    ///
    /// # Arguments
    ///
    /// * `code` - Code region (read-only)
    /// * `ram` - RAM region (read-write)
    /// * `code_start` - Base address for code (typically 0x0)
    /// * `ram_start` - Base address for RAM (typically 0x80000000)
    pub fn new(code: Vec<u8>, ram: Vec<u8>, code_start: u32, ram_start: u32) -> Self {
        Self {
            code,
            ram,
            code_start,
            ram_start,
        }
    }

    /// Create a new memory model with default addresses.
    pub fn with_default_addresses(code: Vec<u8>, ram: Vec<u8>) -> Self {
        Self::new(code, ram, 0x0, DEFAULT_RAM_START)
    }

    /// Read a 32-bit word from memory.
    ///
    /// Returns an error if the address is out of bounds or unaligned.
    pub fn read_word(&self, address: u32) -> Result<i32, EmulatorError> {
        // Check alignment
        if address % 4 != 0 {
            return Err(EmulatorError::UnalignedAccess {
                address,
                alignment: 4,
                pc: 0, // Will be filled in by caller
                regs: [0; 32],
            });
        }

        // Determine which region
        if address >= self.ram_start {
            // RAM region
            let offset = (address - self.ram_start) as usize;
            if offset + 4 > self.ram.len() {
                return Err(EmulatorError::InvalidMemoryAccess {
                    address,
                    size: 4,
                    kind: MemoryAccessKind::Read,
                    pc: 0, // Will be filled in by caller
                    regs: [0; 32],
                });
            }
            let bytes = [
                self.ram[offset],
                self.ram[offset + 1],
                self.ram[offset + 2],
                self.ram[offset + 3],
            ];
            Ok(i32::from_le_bytes(bytes))
        } else {
            // Code region
            let offset = (address - self.code_start) as usize;
            if offset + 4 > self.code.len() {
                return Err(EmulatorError::InvalidMemoryAccess {
                    address,
                    size: 4,
                    kind: MemoryAccessKind::Read,
                    pc: 0, // Will be filled in by caller
                    regs: [0; 32],
                });
            }
            let bytes = [
                self.code[offset],
                self.code[offset + 1],
                self.code[offset + 2],
                self.code[offset + 3],
            ];
            Ok(i32::from_le_bytes(bytes))
        }
    }

    /// Write a 32-bit word to memory.
    ///
    /// Returns an error if the address is out of bounds, unaligned, or in the code region.
    pub fn write_word(&mut self, address: u32, value: i32) -> Result<(), EmulatorError> {
        // Check alignment
        if address % 4 != 0 {
            return Err(EmulatorError::UnalignedAccess {
                address,
                alignment: 4,
                pc: 0, // Will be filled in by caller
                regs: [0; 32],
            });
        }

        // Only allow writes to RAM region
        if address < self.ram_start {
            return Err(EmulatorError::InvalidMemoryAccess {
                address,
                size: 4,
                kind: MemoryAccessKind::Write,
                pc: 0, // Will be filled in by caller
                regs: [0; 32],
            });
        }

        let offset = (address - self.ram_start) as usize;
        if offset + 4 > self.ram.len() {
            return Err(EmulatorError::InvalidMemoryAccess {
                address,
                size: 4,
                kind: MemoryAccessKind::Write,
                pc: 0, // Will be filled in by caller
                regs: [0; 32],
            });
        }

        let bytes = value.to_le_bytes();
        self.ram[offset] = bytes[0];
        self.ram[offset + 1] = bytes[1];
        self.ram[offset + 2] = bytes[2];
        self.ram[offset + 3] = bytes[3];
        Ok(())
    }

    /// Read a 32-bit instruction from the code region.
    ///
    /// Returns an error if the address is out of bounds or unaligned.
    pub fn fetch_instruction(&self, address: u32) -> Result<u32, EmulatorError> {
        // Check alignment
        if address % 4 != 0 {
            return Err(EmulatorError::UnalignedAccess {
                address,
                alignment: 4,
                pc: 0, // Will be filled in by caller
                regs: [0; 32],
            });
        }

        let offset = (address - self.code_start) as usize;
        if offset + 4 > self.code.len() {
            return Err(EmulatorError::InvalidMemoryAccess {
                address,
                size: 4,
                kind: MemoryAccessKind::InstructionFetch,
                pc: 0, // Will be filled in by caller
                regs: [0; 32],
            });
        }

        let bytes = [
            self.code[offset],
            self.code[offset + 1],
            self.code[offset + 2],
            self.code[offset + 3],
        ];
        Ok(u32::from_le_bytes(bytes))
    }

    /// Get a reference to the RAM region (for inspection).
    pub fn ram(&self) -> &[u8] {
        &self.ram
    }

    /// Get a mutable reference to the RAM region (for initialization).
    pub fn ram_mut(&mut self) -> &mut [u8] {
        &mut self.ram
    }

    /// Get a reference to the code region (for debugging).
    pub fn code(&self) -> &[u8] {
        &self.code
    }
}


