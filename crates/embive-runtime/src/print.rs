use core::fmt::{self, Write};

use crate::{syscall, SYSCALL_ARGS};

/// Writer that sends output to the host via syscall
///
/// Syscall 2: Write string to host
/// - args[0] = pointer to string (as i32)
/// - args[1] = length of string
pub struct EmbiveWriter;

impl Write for EmbiveWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        // Syscall 2: Write string to host
        // args[0] = pointer to string (as i32)
        // args[1] = length of string
        let ptr = s.as_ptr() as usize as i32;
        let len = s.len() as i32;

        let mut args = [0i32; SYSCALL_ARGS];
        args[0] = ptr;
        args[1] = len;
        let _ = syscall(2, &args);
        Ok(())
    }
}

/// Global writer instance
static mut WRITER: EmbiveWriter = EmbiveWriter;

/// Print function used by print!/println! macros
///
/// This function is called by the print! and println! macros
/// when used in a no_std environment.
#[no_mangle]
pub fn _print(args: fmt::Arguments) {
    unsafe {
        let _ = WRITER.write_fmt(args);
    }
}

/// Print macro for no_std environments
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::_print(core::format_args!($($arg)*));
    };
}

/// Println macro for no_std environments
#[macro_export]
macro_rules! println {
    () => {
        $crate::print!("\n");
    };
    ($($arg:tt)*) => {
        $crate::print!($($arg)*);
        $crate::print!("\n");
    };
}

