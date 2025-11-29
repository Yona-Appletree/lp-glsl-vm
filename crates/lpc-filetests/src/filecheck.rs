//! Filecheck directive parsing and matching

use alloc::{format, string::String, vec::Vec};

/// Filecheck directive types
#[derive(Debug, Clone)]
pub enum FilecheckDirective {
    /// `check: <pattern>` - Starts a check block
    Check { pattern: String },
    /// `nextln: <content>` - Expects content on next line (strict)
    NextLine { content: String },
    /// `sameln: <content>` - Expects content on same or next line (flexible)
    SameLine { content: String },
    /// End of check block
    EndCheck,
}

/// Parse filecheck directives from expected text
pub fn parse_filecheck_directives(expected_text: &str) -> Vec<FilecheckDirective> {
    let mut directives = Vec::new();
    let lines: Vec<&str> = expected_text.lines().collect();

    for line in lines {
        let trimmed = line.trim();
        if trimmed.starts_with("check:") {
            let pattern = String::from(trimmed[6..].trim());
            directives.push(FilecheckDirective::Check { pattern });
        } else if trimmed.starts_with("nextln:") {
            let content = String::from(trimmed[7..].trim());
            directives.push(FilecheckDirective::NextLine { content });
        } else if trimmed.starts_with("sameln:") {
            let content = String::from(trimmed[7..].trim());
            directives.push(FilecheckDirective::SameLine { content });
        } else if trimmed == "}" {
            directives.push(FilecheckDirective::EndCheck);
        }
    }

    directives
}

/// Match actual output against filecheck directives
pub fn match_filecheck(actual: &str, directives: &[FilecheckDirective]) -> Result<(), String> {
    let actual_lines: Vec<&str> = actual.lines().map(|l| l.trim()).collect();
    let mut actual_idx = 0;
    let mut directive_idx = 0;

    while directive_idx < directives.len() {
        match &directives[directive_idx] {
            FilecheckDirective::Check { pattern } => {
                // Check block starts - verify pattern matches actual output
                if actual_idx >= actual_lines.len() {
                    return Err(format!(
                        "Filecheck failed: check block '{}' started but no more lines in actual output",
                        pattern
                    ));
                }
                // Pattern might be like "domtree_preorder {" or "cfg_postorder:"
                if pattern.ends_with('{') {
                    // Block start - consume the opening line
                    let expected_prefix = pattern.trim_end_matches('{').trim();
                    if !actual_lines[actual_idx].starts_with(expected_prefix) {
                        return Err(format!(
                            "Filecheck failed: expected '{}' but got '{}'",
                            pattern, actual_lines[actual_idx]
                        ));
                    }
                    actual_idx += 1;
                } else if pattern.ends_with(':') {
                    // Single line check - pattern includes colon, actual line should match
                    if actual_lines[actual_idx] != pattern {
                        return Err(format!(
                            "Filecheck failed: expected '{}' but got '{}'",
                            pattern, actual_lines[actual_idx]
                        ));
                    }
                    actual_idx += 1;
                } else {
                    // Pattern without special suffix - exact match
                    if actual_lines[actual_idx] != pattern {
                        return Err(format!(
                            "Filecheck failed: expected '{}' but got '{}'",
                            pattern, actual_lines[actual_idx]
                        ));
                    }
                    actual_idx += 1;
                }
            }
            FilecheckDirective::NextLine { content } => {
                if actual_idx >= actual_lines.len() {
                    return Err(format!(
                        "Filecheck failed: expected '{}' on next line but reached end of output",
                        content
                    ));
                }
                if actual_lines[actual_idx] != content {
                    return Err(format!(
                        "Filecheck failed: expected '{}' but got '{}' (line {})",
                        content, actual_lines[actual_idx], actual_idx + 1
                    ));
                }
                actual_idx += 1;
            }
            FilecheckDirective::SameLine { content } => {
                // Flexible matching: check current line or next few lines (up to 3 lines ahead)
                let mut found = false;
                for offset in 0..=3 {
                    if actual_idx + offset < actual_lines.len()
                        && actual_lines[actual_idx + offset] == content
                    {
                        actual_idx += offset + 1;
                        found = true;
                        break;
                    }
                }
                if !found {
                    return Err(format!(
                        "Filecheck failed: expected '{}' near line {} but got '{}'",
                        content,
                        actual_idx + 1,
                        if actual_idx < actual_lines.len() {
                            actual_lines[actual_idx]
                        } else {
                            "<end of output>"
                        }
                    ));
                }
            }
            FilecheckDirective::EndCheck => {
                // End of check block - verify closing brace if present
                if actual_idx < actual_lines.len() && actual_lines[actual_idx] == "}" {
                    actual_idx += 1;
                }
            }
        }
        directive_idx += 1;
    }

    Ok(())
}

