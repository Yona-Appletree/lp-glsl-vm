#![no_std]
#![no_main]

extern crate alloc;

use alloc::{vec::Vec, string::String};
use core::{panic::PanicInfo, sync::atomic::{AtomicI32, Ordering}};

use embive_runtime::{ebreak, syscall, _print};

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
        $crate::print!("{}\n", core::format_args!($($arg)*));
    };
}

static RESULT: AtomicI32 = AtomicI32::new(0);

/// Format a number as a string (no_std compatible)
fn format_number(mut n: i32) -> String {
    let mut s = String::new();
    let negative = n < 0;
    if negative {
        n = -n;
    }
    
    if n == 0 {
        s.push('0');
    } else {
        let mut digits = Vec::new();
        while n > 0 {
            digits.push((b'0' + (n % 10) as u8) as char);
            n /= 10;
        }
        for &digit in digits.iter().rev() {
            s.push(digit);
        }
    }
    
    if negative {
        let mut result = String::from("-");
        result.push_str(&s);
        result
    } else {
        s
    }
}

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
        
        // Test heap allocation with String
        let mut message = String::from("Sum of ");
        // Format numbers manually since to_string requires std
        let num1_str = format_number(numbers[0]);
        let num2_str = format_number(numbers[1]);
        let result_str = format_number(value);
        message.push_str(&num1_str);
        message.push_str(" and ");
        message.push_str(&num2_str);
        message.push_str(" is ");
        message.push_str(&result_str);
        println!("[guest] {}", message);
        
        // Show Vec contents
        let mut vec_str = String::from("Vec contents: [");
        for (i, &num) in numbers.iter().enumerate() {
            if i > 0 {
                vec_str.push_str(", ");
            }
            vec_str.push_str(&format_number(num));
        }
        vec_str.push(']');
        println!("[guest] {}", vec_str);
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

