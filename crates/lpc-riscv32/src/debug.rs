//! Debug macro for conditional logging.

/// Debug macro that prints in test mode, otherwise is a no-op.
///
/// # Examples
///
/// ```ignore
/// debug!("Value: {}", value);
/// debug!("Multiple values: {} {}", x, y);
/// ```
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        #[cfg(test)]
        {
            extern crate std;
            std::println!($($arg)*);
        }
        #[cfg(not(test))]
        {
            // No-op in non-test builds
            let _ = core::format_args!($($arg)*);
        }
    };
}


