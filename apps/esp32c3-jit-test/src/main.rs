#![no_std]
#![no_main]

extern crate alloc;

use defmt::info;
use embassy_executor::Spawner;
use esp_hal::{clock::CpuClock, timer::systimer::SystemTimer};
use panic_rtt_target as _;
use riscv_shared::build_and_compile_fib_hardware;

// This creates a default app-descriptor required by the esp-idf bootloader.
esp_bootloader_esp_idf::esp_app_desc!();

#[esp_hal_embassy::main]
async fn main(_spawner: Spawner) {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // Allocate heap
    esp_alloc::heap_allocator!(size: 64 * 1024);

    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_hal_embassy::init(timer0.alarm0);

    // Initialize RTT after heap setup
    rtt_target::rtt_init_defmt!();

    info!("ESP32-C6 JIT Test Starting...");

    // Build and compile JIT code
    info!("Building IR and compiling to RISC-V...");
    let jit_result = build_and_compile_fib_hardware();

    info!("Generated {} bytes of RISC-V code", jit_result.code.len());
    info!("ELF file size: {} bytes", jit_result.elf.len());

    // For ESP32-C6, we need to load the code into executable memory
    // The code is already compiled, so we can cast it to a function pointer
    // Note: In a real implementation, we'd need to ensure the memory is executable
    // For now, we'll use a simple approach - the code is in RAM which should be executable

    info!("Loading code into executable memory...");

    // Allocate executable memory for the code from heap
    // ESP32-C6 RAM is executable by default, so we can use heap-allocated memory
    // We need to ensure proper alignment (4 bytes for RISC-V instructions)
    use alloc::vec::Vec;
    let mut code_buffer = Vec::with_capacity(jit_result.code.len());
    code_buffer.extend_from_slice(&jit_result.code);

    // Ensure code is properly aligned (RISC-V instructions are 4-byte aligned)
    let code_ptr = code_buffer.as_ptr();
    if (code_ptr as usize) % 4 != 0 {
        defmt::panic!("Code buffer not properly aligned");
    }

    unsafe {
        // Flush instruction cache to ensure code is visible
        // ESP32-C6 uses instruction cache, so we need to sync
        core::arch::asm!("fence.i");

        // Cast to function pointer
        // Note: code_buffer must stay alive during the function call
        // Bootstrap function signature: extern "C" fn() -> i32
        // It calls fib(10) and returns the result (55)
        type FibBootstrapFunc = extern "C" fn() -> i32;
        let fib_func: FibBootstrapFunc = core::mem::transmute(code_ptr);

        // Test the function
        let n = 10;
        let expected = 55; // fib(10) = 55
        info!("Calling JIT function: bootstrap (calls fib({}))", n);
        info!("Expected result: {}", expected);

        let result = fib_func();
        info!("JIT function returned: {}", result);
        info!("Expected: {}, Got: {}", expected, result);

        if result == expected {
            info!("===== JIT TEST SUCCESS! =====");
        } else {
            defmt::panic!("JIT test failed: expected {}, got {}", expected, result);
        }
    }

    info!("Test completed successfully!");

    // Loop forever
    loop {
        embassy_time::Timer::after(embassy_time::Duration::from_secs(1)).await;
    }
}
