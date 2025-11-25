//! Test builder for R5 functions.

extern crate alloc;

use alloc::{string::String, vec::Vec};

use r5_builder::FunctionBuilder;
use r5_ir::{Function, Module, Signature, Type};
use r5_target_riscv32::{compile_module, debug_elf, generate_elf};
use riscv32_encoder::disassemble_code;

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
        module.add_function(test_func_name.clone(), test_func.clone());

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
}
