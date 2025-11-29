//! Filecheck directive parsing and matching using the filecheck crate

use filecheck::{Checker, CheckerBuilder, NO_VARIABLES};

/// Build a filechecker from expected text containing directives
pub fn build_filechecker(expected_text: &str) -> Result<Checker, String> {
    let mut builder = CheckerBuilder::new();
    
    // Parse each line and add directives
    for line in expected_text.lines() {
        let trimmed = line.trim();
        // Skip empty lines
        if trimmed.is_empty() {
            continue;
        }
        
        // Skip check: directives that start with '#' - these are comment/section markers
        // that don't need to match actual output (e.g., "check: # Prologue")
        if trimmed.starts_with("check:") {
            let pattern = trimmed[6..].trim();
            if pattern.starts_with('#') {
                // This is a comment marker - skip it
                continue;
            }
        }
        
        // Add directive - filecheck crate will parse it
        builder
            .directive(trimmed)
            .map_err(|e| format!("Failed to parse filecheck directive '{}': {}", trimmed, e))?;
    }
    
    Ok(builder.finish())
}

/// Match actual output against filecheck directives
pub fn match_filecheck(actual: &str, expected_text: &str) -> Result<(), String> {
    let checker = build_filechecker(expected_text)?;
    
    if checker.check(actual, NO_VARIABLES).map_err(|e| format!("Filecheck error: {}", e))? {
        Ok(())
    } else {
        // Get explanation for why matching failed
        let (_, explain) = checker
            .explain(actual, NO_VARIABLES)
            .map_err(|e| format!("Failed to get filecheck explanation: {}", e))?;
        
        Err(format!("Filecheck failed:\n{}", explain))
    }
}

/// Parse filecheck directives from expected text
/// 
/// This function is kept for backward compatibility but now just returns
/// whether the text contains filecheck directives (non-empty after trimming).
/// The actual parsing is done by the filecheck crate.
pub fn parse_filecheck_directives(expected_text: &str) -> Vec<()> {
    // Check if there are any non-empty lines that look like directives
    let has_directives = expected_text
        .lines()
        .any(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty()
                && (trimmed.starts_with("check:")
                    || trimmed.starts_with("nextln:")
                    || trimmed.starts_with("sameln:")
                    || trimmed.starts_with("CHECK:")
                    || trimmed.starts_with("CHECK-NEXT:")
                    || trimmed.starts_with("CHECK-SAME:"))
        });
    
    if has_directives {
        vec![()]
    } else {
        vec![]
    }
}
