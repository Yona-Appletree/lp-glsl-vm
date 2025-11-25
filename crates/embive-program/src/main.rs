#![no_std]
#![no_main]

extern crate alloc;
#[macro_use]
extern crate embive_runtime;

use core::panic::PanicInfo;

use embive_runtime::{ebreak, panic_syscall};
mod jit_test;

/// Panics will report to the host VM and then exit
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // In no_std, we can't easily format the panic message
    // Use a static message and try to extract location info
    let msg = b"panic occurred\0";

    // Try to extract location info
    let (file_ptr, file_len, line) = if let Some(loc) = info.location() {
        let file = loc.file().as_bytes();
        (file.as_ptr(), file.len(), loc.line())
    } else {
        (core::ptr::null(), 0, 0)
    };

    // Report panic to host VM
    panic_syscall(msg.as_ptr(), msg.len() - 1, file_ptr, file_len, line);
}

/// Interrupt handler
/// This function is called when an interruption occurs
#[no_mangle]
fn interrupt_handler(_value: i32) {
    // Handle interrupts if needed
}

/// Main program that runs the JIT experiment
#[no_mangle]
pub extern "Rust" fn main() {
    println!("[guest] Hello!");

    // Run JIT experiment
    jit_test::jit_add_experiment();

    // Exit
    ebreak()
}
