//! ABI (Application Binary Interface) implementation for RISC-V 32-bit.
//!
//! This module handles argument and return value passing according to the
//! RISC-V 32-bit calling convention.

/// ABI information for a function.
///
/// This tracks how arguments and return values are passed according to
/// the RISC-V 32-bit ABI.
#[derive(Clone, Debug)]
pub struct Abi {
    /// Number of arguments passed in registers (max 8 for RV32).
    pub num_reg_args: usize,

    /// Number of arguments passed on the stack.
    pub num_stack_args: usize,

    /// Number of return values passed in registers (max 2 for RV32).
    pub num_reg_rets: usize,

    /// Number of return values passed on the stack (multi-return).
    pub num_stack_rets: usize,

    /// Size of stack arguments in bytes.
    pub stack_args_size: u32,

    /// Size of stack return values in bytes.
    pub stack_rets_size: u32,

    /// Whether multi-return is enabled (requires return area).
    pub uses_return_area: bool,
}

impl Abi {
    /// Compute ABI information for a function signature.
    ///
    /// # Parameters
    ///
    /// - `num_params`: Number of function parameters
    /// - `num_returns`: Number of return values
    /// - `enable_multi_ret`: Whether multi-return is enabled
    ///
    /// # Returns
    ///
    /// ABI information describing how arguments and returns are passed.
    pub fn compute_abi_info(
        num_params: usize,
        num_returns: usize,
        enable_multi_ret: bool,
    ) -> Self {
        // RISC-V 32-bit: 8 argument registers (a0-a7, x10-x17)
        const MAX_REG_ARGS: usize = 8;
        // RISC-V 32-bit: 2 return registers (a0-a1, x10-x11)
        const MAX_REG_RETS: usize = 2;

        let num_reg_args = num_params.min(MAX_REG_ARGS);
        let num_stack_args = num_params.saturating_sub(MAX_REG_ARGS);

        // Each argument is 4 bytes (i32)
        let stack_args_size = (num_stack_args as u32) * 4;
        // Align stack args to 16 bytes
        let stack_args_size = (stack_args_size + 15) & !15;

        // Return values
        let num_reg_rets = if enable_multi_ret {
            num_returns.min(MAX_REG_RETS)
        } else {
            // Without multi-return, only 2 returns allowed
            if num_returns > MAX_REG_RETS {
                panic!("Too many return values: {} (max {})", num_returns, MAX_REG_RETS);
            }
            num_returns
        };

        let num_stack_rets = if enable_multi_ret {
            num_returns.saturating_sub(MAX_REG_RETS)
        } else {
            0
        };

        // Each return value is 4 bytes (i32)
        let stack_rets_size = (num_stack_rets as u32) * 4;
        // Align stack returns to 16 bytes
        let stack_rets_size = (stack_rets_size + 15) & !15;

        let uses_return_area = num_stack_rets > 0;

        Self {
            num_reg_args,
            num_stack_args,
            num_reg_rets,
            num_stack_rets,
            stack_args_size,
            stack_rets_size,
            uses_return_area,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_abi_simple_args() {
        let abi = Abi::compute_abi_info(3, 1, false);
        assert_eq!(abi.num_reg_args, 3);
        assert_eq!(abi.num_stack_args, 0);
        assert_eq!(abi.num_reg_rets, 1);
        assert_eq!(abi.num_stack_rets, 0);
        assert!(!abi.uses_return_area);
    }

    #[test]
    fn test_abi_many_args() {
        let abi = Abi::compute_abi_info(10, 1, false);
        assert_eq!(abi.num_reg_args, 8);
        assert_eq!(abi.num_stack_args, 2);
        assert_eq!(abi.stack_args_size, 16); // 2 args * 4 bytes = 8, aligned to 16
    }

    #[test]
    fn test_abi_multi_return() {
        let abi = Abi::compute_abi_info(0, 5, true);
        assert_eq!(abi.num_reg_rets, 2);
        assert_eq!(abi.num_stack_rets, 3);
        assert_eq!(abi.stack_rets_size, 16); // 3 rets * 4 bytes = 12, aligned to 16
        assert!(abi.uses_return_area);
    }

    #[test]
    fn test_abi_max_reg_args() {
        let abi = Abi::compute_abi_info(8, 0, false);
        assert_eq!(abi.num_reg_args, 8);
        assert_eq!(abi.num_stack_args, 0);
    }

    #[test]
    fn test_abi_max_reg_rets() {
        let abi = Abi::compute_abi_info(0, 2, false);
        assert_eq!(abi.num_reg_rets, 2);
        assert_eq!(abi.num_stack_rets, 0);
    }
}

