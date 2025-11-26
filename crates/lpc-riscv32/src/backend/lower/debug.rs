//! Debug logging infrastructure for instruction lowering.
//!
//! This module provides feature-gated debug logging that compiles to nothing
//! when the `debug-lowering` feature is disabled, ensuring zero runtime cost
//! in production builds.

/// Debug logging macro that compiles to nothing when `debug-lowering` feature is disabled.
///
/// # Examples
///
/// ```ignore
/// debug_lowering!("FrameLayout::compute: setup_area={}, clobber={}, spills={}, total={}",
///                 setup_area, clobber, spills, total);
/// debug_lowering!("spill_slot_offset(slot={}): offset={}", slot, offset.as_i32());
/// ```
#[cfg(feature = "debug-lowering")]
#[macro_export]
macro_rules! debug_lowering {
    ($($arg:tt)*) => {
        {
            // Use core::fmt for no_std compatibility
            // In tests, this will print to stderr via std::eprintln!
            #[cfg(test)]
            {
                extern crate std;
                std::eprintln!("[DEBUG] {}", core::format_args!($($arg)*));
            }
            // In non-test builds, we could use a custom logger if needed
            // For now, compile to nothing in non-test builds
            #[cfg(not(test))]
            {
                let _ = core::format_args!($($arg)*);
            }
        }
    };
}

/// Debug logging macro that compiles to nothing when `debug-lowering` feature is disabled.
#[cfg(not(feature = "debug-lowering"))]
#[macro_export]
macro_rules! debug_lowering {
    ($($arg:tt)*) => {
        // Compile to nothing when feature is disabled
    };
}

