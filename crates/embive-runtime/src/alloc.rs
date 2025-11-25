use core::alloc::Layout;
use core::ptr::addr_of;

use linked_list_allocator::LockedHeap;

use crate::get_heap;

/// Get the end of heap address from linker script
///
/// Returns the end address of the heap region (__heap_end).
fn get_heap_end() -> usize {
    extern "C" {
        static __heap_end: u8;
    }
    
    // __heap_end is a linker symbol pointing to the end of the heap section
    addr_of!(__heap_end) as usize
}

/// Global allocator instance
#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

/// Initialize the global allocator
///
/// This must be called before any heap allocations are made.
/// It sets up the allocator to use the memory region from `_end` to a reasonable heap end.
/// We limit the heap to 1MB to avoid issues with VM memory constraints.
pub unsafe fn init_allocator() {
    let heap_start = get_heap();
    
    // Ensure heap_start is in RAM region (>= RAM_OFFSET = 0x80000000)
    // If it's not, something is wrong with the linker script
    const RAM_OFFSET: usize = 0x80000000;
    if heap_start < RAM_OFFSET {
        // Heap start is not in RAM, skip initialization
        return;
    }

    // Calculate heap size from linker script symbols
    // Use __heap_end from the linker script, which marks the end of the heap section
    let heap_end_addr = get_heap_end();
    let heap_size = if heap_end_addr > heap_start {
        heap_end_addr - heap_start
    } else {
        // Fallback: use a conservative 512KB if symbols are invalid
        512 * 1024
    };
    
    // Ensure we have a valid heap size
    if heap_size == 0 {
        // No heap available, allocator will fail gracefully
        return;
    }
    
    // Initialize the allocator with the heap region
    // The allocator will manage memory starting at heap_start
    // Ensure heap_start is properly aligned (16 bytes for linked_list_allocator)
    // linked_list_allocator requires 16-byte alignment
    let aligned_heap_start = (heap_start + 15) & !15;
    
    // Ensure the heap start is at or after RAM_OFFSET
    let safe_heap_start = aligned_heap_start.max(RAM_OFFSET);
    
    // Recalculate heap size based on the safe start
    let safe_heap_size = if safe_heap_start > heap_start {
        heap_size - (safe_heap_start - heap_start)
    } else {
        heap_size
    };
    
    if safe_heap_size < 256 {
        // Not enough space after safety margin
        return;
    }
    
    // Initialize the allocator
    // The allocator will write to memory starting at safe_heap_start
    // NOTE: safe_heap_start is a guest virtual address. When the allocator
    // tries to write to memory, those writes go through the interpreter's
    // memory abstraction (SliceMemory), which translates guest addresses to
    // offsets in the VM's RAM buffer.
    ALLOCATOR.lock().init(safe_heap_start as *mut u8, safe_heap_size);
}

/// Custom allocator error handler
///
/// Called when allocation fails. In a no_std environment, we can't do much,
/// but this prevents undefined behavior.
#[alloc_error_handler]
fn alloc_error_handler(_layout: Layout) -> ! {
    // In a no_std environment, we can't print or panic properly
    // Just abort by calling ebreak
    crate::ebreak()
}

