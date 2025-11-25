#![no_std]
#![no_main]

extern crate alloc;
#[macro_use]
extern crate embive_runtime;

use alloc::vec::Vec;
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
/// Syscall 1000: Add two numbers (args[0] + args[1])
/// Syscall 0: Done - signals completion with result value
#[no_mangle]
pub extern "Rust" fn main() {
    println!("[guest] Hello!");
    
    // Test heap allocation with Vec
    let mut numbers = Vec::new();
    numbers.push(5);
    numbers.push(10);
    numbers.push(15);
    
    println!("[guest] Created Vec with {} elements", numbers.len());
    
    // System Call 1000: Add two numbers (5 + 10 = 15)
    let result = syscall(1000, &[numbers[0], numbers[1], 0, 0, 0, 0, 0]);
    
    if let Ok(value) = result {
        RESULT.store(value, Ordering::SeqCst);
        
        println!("[guest] Sum of {} and {} is {}", numbers[0], numbers[1], value);
        
        // Show Vec contents
        print!("[guest] Vec contents: [");
        for (i, &num) in numbers.iter().enumerate() {
            if i > 0 {
                print!(", ");
            }
            print!("{}", num);
        }
        println!("]");
    }
    
    // Test multiple allocations
    let mut allocated_vecs = Vec::new();
    for i in 0..5 {
        let mut vec = Vec::new();
        for j in 0..(i + 1) {
            vec.push(j * 10);
        }
        allocated_vecs.push(vec);
    }
    println!("[guest] Created {} Vecs with various sizes", allocated_vecs.len());
    
    // Verify allocations work by summing all values
    let mut total = 0;
    for vec in &allocated_vecs {
        for &val in vec {
            total += val;
        }
    }
    println!("[guest] Sum of all allocated values: {}", total);
    
    // Signal completion with result 42
    let _ = syscall(0, &[42, 0, 0, 0, 0, 0, 0]);
    
    // Exit
    ebreak()
}

