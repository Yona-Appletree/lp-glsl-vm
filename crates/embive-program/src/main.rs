#![no_std]
#![no_main]

use core::{panic::PanicInfo, sync::atomic::{AtomicI32, Ordering}};

use embive_runtime::{ebreak, syscall};

static RESULT: AtomicI32 = AtomicI32::new(0);

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

/// Simple program that adds two numbers using a syscall
/// Syscall 1: Add two numbers (args[0] + args[1])
#[no_mangle]
pub extern "Rust" fn main() {
    // System Call 1: Add two numbers (5 + 10 = 15)
    let result = syscall(1, &[5, 10, 0, 0, 0, 0, 0]);
    
    if let Ok(value) = result {
        RESULT.store(value, Ordering::SeqCst);
    }
    
    // Exit
    ebreak()
}

