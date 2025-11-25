use core::num::NonZeroI32;
use std::io::Write;

use embive::{
    interpreter::{
        memory::{SliceMemory, RAM_OFFSET},
        Error, Interpreter, State, SYSCALL_ARGS,
    },
    transpiler::transpile_elf,
};

/// RISC-V VM for running embive programs
pub struct R5Vm {
    code_vec: Vec<u8>,
    ram: Vec<u8>,
    last_result: Option<i32>,
}

impl R5Vm {
    /// Create a new R5Vm with the specified RAM size
    pub fn new(ram_size: usize) -> Self {
        Self {
            code_vec: Vec::new(),
            ram: vec![0u8; ram_size],
            last_result: None,
        }
    }

    /// Load an ELF binary into the VM
    pub fn load(&mut self, elf_data: &[u8]) -> Result<(), String> {
        // Transpile ELF to embive bytecode
        // Use 4MB buffer to ensure we can handle large binaries
        const MAX_BINARY_SIZE: usize = 4 * 1024 * 1024;
        let mut combined = vec![0u8; MAX_BINARY_SIZE];
        let binary_size = transpile_elf(elf_data, &mut combined)
            .map_err(|e| format!("Failed to transpile ELF: {:?}", e))?;

        // Split ROM (low addresses) and RAM (high addresses)
        let code_size = RAM_OFFSET.min(binary_size as u32) as usize;
        // Load the full code section - allocate enough space for the entire code section
        // Ensure minimum size to avoid zero-sized allocation issues
        self.code_vec = vec![0u8; code_size.max(1)];
        if code_size > 0 {
            self.code_vec[..code_size].copy_from_slice(&combined[..code_size]);
        }

        // Copy RAM sections if any
        if binary_size > code_size {
            let ram_offset_in_combined = code_size;
            let ram_size = (binary_size - ram_offset_in_combined).min(self.ram.len());
            self.ram[..ram_size].copy_from_slice(
                &combined[ram_offset_in_combined..ram_offset_in_combined + ram_size],
            );
        }

        // Ensure the heap region is zero-initialized
        // The .heap section is (NOLOAD) so it won't be in the binary, but we need
        // to ensure the RAM buffer covers it. The RAM is already zero-initialized
        // in new(), so the heap region should be ready for use.
        // Note: The heap starts at _end (after .data) and extends to __heap_end.
        // Since the RAM buffer is 4MB and heap is 512KB, there should be plenty of space.

        Ok(())
    }

    /// Run the VM until it halts
    pub fn run(&mut self) -> Result<(), String> {
        // Capture what we need for syscall handling before creating mutable borrows
        let last_result = &mut self.last_result;
        let code_vec_ptr = self.code_vec.as_ptr();
        let code_vec_len = self.code_vec.len();
        let ram_ptr = self.ram.as_ptr();
        let ram_len = self.ram.len();

        // Create memory and interpreter
        let mut memory = SliceMemory::new(&self.code_vec, &mut self.ram);
        let mut interpreter = Interpreter::new(&mut memory, 0);
        interpreter.program_counter = 0;

        // Syscall handler - inline the logic from handle_syscall
        let mut syscall = |nr: i32,
                           args: &[i32; SYSCALL_ARGS],
                           _memory: &mut _|
         -> Result<Result<i32, NonZeroI32>, Error> {
            match nr {
                0 => {
                    // Syscall 0: Done - store result
                    *last_result = Some(args[0]);
                    Ok(Ok(0))
                }
                2 => {
                    // Syscall 2: Write string to host
                    let ptr = args[0] as u32;
                    let len = args[1] as usize;

                    // Read string from guest memory
                    let mut buf = vec![0u8; len];
                    unsafe {
                        if ptr < RAM_OFFSET {
                            let offset = ptr as usize;
                            if offset + len > code_vec_len {
                                return Err(Error::Custom("Address out of bounds in code section"));
                            }
                            core::ptr::copy_nonoverlapping(
                                code_vec_ptr.add(offset),
                                buf.as_mut_ptr(),
                                len,
                            );
                        } else {
                            let offset = (ptr - RAM_OFFSET) as usize;
                            if offset + len > ram_len {
                                return Err(Error::Custom("Address out of bounds in RAM section"));
                            }
                            core::ptr::copy_nonoverlapping(
                                ram_ptr.add(offset),
                                buf.as_mut_ptr(),
                                len,
                            );
                        }
                    }

                    // Convert to string and print
                    let s = core::str::from_utf8(&buf)
                        .map_err(|_| Error::Custom("Invalid UTF-8 string"))?;
                    #[cfg(feature = "std")]
                    {
                        print!("{}", s);
                        std::io::stdout().flush().ok();
                    }
                    #[cfg(not(feature = "std"))]
                    {
                        // In no_std, we can't print, so just ignore
                    }

                    Ok(Ok(0))
                }
                1000 => {
                    // Syscall 1000: Add two numbers
                    Ok(Ok(args[0] + args[1]))
                }
                _ => Err(Error::Custom("Unknown syscall")),
            }
        };

        // Run the program (exactly like embive examples)
        loop {
            match interpreter
                .run()
                .map_err(|e| format!("Interpreter error: {:?}", e))?
            {
                State::Running => {}
                State::Called => {
                    interpreter
                        .syscall(&mut syscall)
                        .map_err(|e| format!("Syscall error: {:?}", e))?;
                }
                State::Waiting => {}
                State::Halted => break,
            }
        }

        Ok(())
    }

    /// Get the last result from syscall 0 (done)
    pub fn last_result(&self) -> Option<i32> {
        self.last_result
    }

    /// Handle a syscall from the guest program
    ///
    /// Supported syscalls:
    /// - 0: Done - stores args[0] in last_result
    /// - 2: Write - reads string from memory at args[0] with length args[1] and prints it
    /// - 1000: Add - returns args[0] + args[1]
    pub fn handle_syscall(&mut self, nr: i32, args: &[i32; SYSCALL_ARGS]) -> Result<i32, Error> {
        match nr {
            0 => {
                // Syscall 0: Done - store result
                self.last_result = Some(args[0]);
                Ok(0)
            }
            2 => {
                // Syscall 2: Write string to host
                // args[0] = pointer to string (as i32)
                // args[1] = length of string
                let ptr = args[0] as u32;
                let len = args[1] as usize;

                // Read string from guest memory
                let buf = self.read_memory(ptr, len)?;

                // Convert to string and print
                let s = core::str::from_utf8(&buf)
                    .map_err(|_| Error::Custom("Invalid UTF-8 string"))?;
                #[cfg(feature = "std")]
                {
                    print!("{}", s);
                    std::io::stdout().flush().ok();
                }
                #[cfg(not(feature = "std"))]
                {
                    // In no_std, we can't print, so just ignore
                }

                Ok(0)
            }
            1000 => {
                // Syscall 1000: Add two numbers
                Ok(args[0] + args[1])
            }
            _ => Err(Error::Custom("Unknown syscall")),
        }
    }

    /// Read bytes from guest memory at the specified address
    ///
    /// Addresses < RAM_OFFSET are in the code section (ROM), addresses >= RAM_OFFSET are in RAM.
    pub fn read_memory(&self, addr: u32, len: usize) -> Result<Vec<u8>, Error> {
        let mut buf = vec![0u8; len];
        unsafe {
            if addr < RAM_OFFSET {
                // Read from code section (ROM)
                let offset = addr as usize;
                if offset + len > self.code_vec.len() {
                    return Err(Error::Custom("Address out of bounds in code section"));
                }
                core::ptr::copy_nonoverlapping(
                    self.code_vec.as_ptr().add(offset),
                    buf.as_mut_ptr(),
                    len,
                );
            } else {
                // Read from RAM section
                let offset = (addr - RAM_OFFSET) as usize;
                if offset + len > self.ram.len() {
                    return Err(Error::Custom("Address out of bounds in RAM section"));
                }
                core::ptr::copy_nonoverlapping(
                    self.ram.as_ptr().add(offset),
                    buf.as_mut_ptr(),
                    len,
                );
            }
        }
        Ok(buf)
    }

    /// Write bytes to guest memory at the specified address
    ///
    /// Addresses < RAM_OFFSET are in the code section (ROM, read-only), addresses >= RAM_OFFSET are in RAM.
    /// Returns an error if trying to write to ROM or if address is out of bounds.
    pub fn write_memory(&mut self, addr: u32, data: &[u8]) -> Result<(), Error> {
        if addr < RAM_OFFSET {
            return Err(Error::Custom("Cannot write to ROM section"));
        }

        let offset = (addr - RAM_OFFSET) as usize;
        if offset + data.len() > self.ram.len() {
            return Err(Error::Custom("Address out of bounds in RAM section"));
        }

        unsafe {
            core::ptr::copy_nonoverlapping(
                data.as_ptr(),
                self.ram.as_mut_ptr().add(offset),
                data.len(),
            );
        }

        Ok(())
    }
}
