#![no_std]
#![no_main]

extern crate alloc;
#[macro_use]
extern crate embive_runtime;

use core::panic::PanicInfo;

use embive_runtime::ebreak;
mod jit_test;

/// Panics will simply exit the interpreter (ebreak)
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    ebreak()
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
