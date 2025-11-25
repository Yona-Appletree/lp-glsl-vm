//! Test builder for R5 functions.

extern crate alloc;

use alloc::{string::String, vec::Vec};

use r5_builder::FunctionBuilder;
use r5_ir::{Function, Inst, Module, Signature, Type, Value};
use r5_target_riscv32::{compile_module, debug_elf, generate_elf};
use riscv32_encoder::{disassemble_code, Gpr};

use crate::vm_runner::{Expectation, VmRunner};

/// Builder for testing R5 IR functions.
///
/// # Example
///
/// ```rust
/// use r5_test_util::R5FnTest;
///
/// let func = build_my_function();
/// R5FnTest::new(func)
///     .with_args(&[5, 10])
///     .expect_return(15)
///     .run();
/// ```
pub struct R5FnTest {
    func: Function,
    args: Vec<i32>,
    vm_ram_size: usize,
    expectations: Vec<Expectation>,
    debug_enabled: bool,
}

impl R5FnTest {
    /// Create a new test for the given function.
    pub fn new(func: Function) -> Self {
        Self {
            func,
            args: Vec::new(),
            vm_ram_size: 4 * 1024 * 1024, // 4MB default
            expectations: Vec::new(),
            debug_enabled: false,
        }
    }

    /// Enable or disable debug output.
    pub fn debug(mut self, enable: bool) -> Self {
        self.debug_enabled = enable;
        self
    }

    /// Set function arguments.
    ///
    /// # Example
    ///
    /// ```rust
    /// R5FnTest::new(func).with_args(&[5, 10]).run();
    /// ```
    pub fn with_args(mut self, args: &[i32]) -> Self {
        self.args = args.to_vec();
        self
    }

    /// Expect a single return value.
    ///
    /// # Example
    ///
    /// ```rust
    /// R5FnTest::new(func)
    ///     .with_args(&[5, 10])
    ///     .expect_return(15)
    ///     .run();
    /// ```
    pub fn expect_return(mut self, value: i32) -> Self {
        self.expectations.push(Expectation::ReturnValue(value));
        self
    }

    /// Expect multiple return values.
    ///
    /// # Example
    ///
    /// ```rust
    /// R5FnTest::new(func)
    ///     .with_args(&[5, 10])
    ///     .expect_return_values(vec![15, 0])
    ///     .run();
    /// ```
    pub fn expect_return_values(mut self, values: Vec<i32>) -> Self {
        self.expectations.push(Expectation::ReturnValues(values));
        self
    }

    /// Expect a panic with optional message.
    ///
    /// # Example
    ///
    /// ```rust
    /// R5FnTest::new(func)
    ///     .with_args(10, 0)
    ///     .expect_panic("division by zero")
    ///     .run();
    /// ```
    pub fn expect_panic(mut self, message: impl Into<String>) -> Self {
        self.expectations.push(Expectation::Panic {
            message: Some(message.into()),
        });
        self
    }

    /// Expect no panic.
    ///
    /// # Example
    ///
    /// ```rust
    /// R5FnTest::new(func).with_args(10, 2).expect_no_panic().run();
    /// ```
    pub fn expect_no_panic(mut self) -> Self {
        self.expectations.push(Expectation::NoPanic);
        self
    }

    /// Expect a memory value at a specific address.
    ///
    /// # Example
    ///
    /// ```rust
    /// R5FnTest::new(func)
    ///     .with_args(&[42])
    ///     .expect_memory_at(0x80000000, &[0x2a, 0x00, 0x00, 0x00])
    ///     .run();
    /// ```
    pub fn expect_memory_at(mut self, address: u32, value: &[u8]) -> Self {
        self.expectations.push(Expectation::Memory {
            address,
            value: value.to_vec(),
        });
        self
    }

    /// Set VM RAM size.
    ///
    /// # Example
    ///
    /// ```rust
    /// R5FnTest::new(func)
    ///     .vm_ram_size(8 * 1024 * 1024) // 8MB
    ///     .run();
    /// ```
    pub fn vm_ram_size(mut self, size: usize) -> Self {
        self.vm_ram_size = size;
        self
    }

    /// Run the test and assert all expectations.
    ///
    /// # Panics
    ///
    /// Panics if any expectation fails or if the test cannot be run.
    pub fn run(self) {
        // Create a module with the test function
        let mut module = Module::new();
        let test_func_name = "test_function".to_string();
        let mut test_func = self.func.clone();
        test_func.set_name(test_func_name.clone());
        module.add_function(test_func_name.clone(), test_func);

        // Generate bootstrap wrapper function in IR
        let wrapper_func =
            Self::generate_bootstrap_function(&self.func, &self.args, &test_func_name);
        let wrapper_name = "bootstrap".to_string();
        module.add_function(wrapper_name.clone(), wrapper_func);
        module.set_entry_function(wrapper_name);

        // Compile module to RISC-V code
        let compiled_code = compile_module(&module);

        // Generate ELF file
        let elf_data = generate_elf(&compiled_code);

        // Run in VM
        let mut runner = VmRunner::new(self.vm_ram_size);
        let result = runner.run(&elf_data, &self.args);

        // Check if test failed
        let test_failed = match &result {
            Ok(r) => self
                .expectations
                .iter()
                .any(|exp| exp.check(r, &self.func, &self.args).is_err()),
            Err(_) => true,
        };

        // Print debug info if test failed or debug enabled
        if test_failed || self.debug_enabled {
            eprintln!("\n=== IR Debug Info ===");
            eprintln!("{}", module);

            eprintln!("\n=== Compiled RISC-V Code ===");
            eprintln!("{}", disassemble_code(&compiled_code));

            eprintln!("\n=== ELF Debug Info ===");
            eprintln!("{}", debug_elf(&elf_data));
        }

        // Handle result
        let result = result.unwrap_or_else(|e| {
            panic!("Failed to run test in VM: {}", e);
        });

        // Check all expectations
        for expectation in &self.expectations {
            expectation
                .check(&result, &self.func, &self.args)
                .unwrap_or_else(|msg| {
                    panic!(
                        "Test expectation failed: {}\n  Function: {:?}\n  Arguments: {:?}",
                        msg, self.func, self.args
                    );
                });
        }
    }

    /// Generate bootstrap wrapper function in IR that calls the test function.
    ///
    /// The bootstrap function:
    /// 1. Sets up function arguments using iconst instructions
    /// 2. Calls the test function using Call instruction
    /// 3. Returns the result (which will be used by syscall)
    fn generate_bootstrap_function(
        test_func: &Function,
        args: &[i32],
        test_func_name: &str,
    ) -> Function {
        // Bootstrap signature: no params, returns i32 (for syscall)
        let sig = Signature::new(Vec::new(), vec![Type::I32]);
        let mut builder = FunctionBuilder::new(sig);

        let block_idx = builder.create_block();

        // Create argument values using iconst
        let mut arg_values = Vec::new();
        for &arg in args.iter().take(8) {
            let arg_val = builder.new_value();
            {
                let mut block_builder = builder.block_builder(block_idx);
                block_builder.iconst(arg_val, arg as i64);
            }
            arg_values.push(arg_val);
        }

        // Call the test function
        let result_val = builder.new_value();
        {
            let mut block_builder = builder.block_builder(block_idx);
            block_builder.call(test_func_name.to_string(), arg_values, vec![result_val]);
        }

        // Call syscall 0 with the result value
        {
            let mut block_builder = builder.block_builder(block_idx);
            block_builder.syscall(0, vec![result_val]);
        }

        // Halt execution
        {
            let mut block_builder = builder.block_builder(block_idx);
            block_builder.halt();
        }

        builder.finish()
    }

    /// Generate bootstrap code that wraps the test function (legacy method, deprecated).
    ///
    /// The bootstrap:
    /// 1. Sets up function arguments in a0, a1, etc.
    /// 2. Calls the test function (using jal)
    /// 3. Takes return value from a0 and calls syscall 0
    #[allow(dead_code)]
    fn generate_bootstrap(func_code: &[u8], args: &[i32]) -> Vec<u8> {
        use r5_target_riscv32::CodeBuffer;
        use riscv32_encoder;

        let mut bootstrap = CodeBuffer::new();

        // Set up function arguments in a0, a1, a2, etc.
        // For now, we'll use constants (lui + addi for large values)
        for (i, &arg) in args.iter().take(8).enumerate() {
            let reg = match i {
                0 => Gpr::A0,
                1 => Gpr::A1,
                2 => Gpr::A2,
                3 => Gpr::A3,
                4 => Gpr::A4,
                5 => Gpr::A5,
                6 => Gpr::A6,
                7 => Gpr::A7,
                _ => break,
            };

            if arg >= -2048 && arg < 2048 {
                // Small immediate, use addi
                bootstrap.emit(riscv32_encoder::addi(reg, Gpr::ZERO, arg));
            } else {
                // Large immediate, use lui + addi
                let arg_u32 = arg as u32;
                let imm_hi = (arg_u32 >> 12) & 0xfffff;
                let imm_lo = (arg_u32 & 0xfff) as i32;
                let imm_lo_signed = if imm_lo & 0x800 != 0 {
                    imm_lo | (-4096i32)
                } else {
                    imm_lo
                };

                bootstrap.emit(riscv32_encoder::lui(reg, imm_hi << 12));
                if imm_lo_signed != 0 {
                    bootstrap.emit(riscv32_encoder::addi(reg, reg, imm_lo_signed));
                }
            }
        }

        // Store bootstrap size before emitting jal
        let bootstrap_before_jal = bootstrap.len();

        // Emit placeholder jal (will be fixed up)
        // jal ra, offset - sets ra = pc + 4, then jumps to pc + offset
        bootstrap.emit(0); // Placeholder

        // After function returns, a0 contains the return value
        // Call syscall 0: set a7 = 0, then ecall
        bootstrap.emit(riscv32_encoder::addi(Gpr::A7, Gpr::ZERO, 0));
        bootstrap.emit(riscv32_encoder::ecall());

        // Halt loop (shouldn't be reached, but ensures program stops)
        bootstrap.emit(riscv32_encoder::jal(Gpr::ZERO, -4));

        // Append the function code
        let mut result = bootstrap.as_bytes().to_vec();
        result.extend_from_slice(func_code);

        // Fix up jal offset
        // jal is PC-relative: pc = pc + offset
        // When jal executes, PC points to the jal instruction
        // We want to jump to the function code start
        // offset = target_address - jal_address
        // target_address = bootstrap.len() (where function code starts in result)
        // jal_address = bootstrap_before_jal (where jal instruction is in result)
        let func_start = bootstrap.len();
        let jal_address = bootstrap_before_jal;
        let jal_offset = func_start as i32 - jal_address as i32;

        // Update the jal instruction
        let jal_inst = riscv32_encoder::jal(Gpr::RA, jal_offset);
        let jal_bytes = jal_inst.to_le_bytes();
        result[bootstrap_before_jal..bootstrap_before_jal + 4].copy_from_slice(&jal_bytes);

        result
    }
}
