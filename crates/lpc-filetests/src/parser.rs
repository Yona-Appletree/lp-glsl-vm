//! Test file parsing

/// A test case extracted from a test file
#[derive(Debug, Clone)]
pub struct TestCase {
    /// The function text
    pub function_text: String,
    /// The expected output text (from comments) or annotations
    pub expected_text: String,
    /// The test command type
    pub command: String,
}

/// Parse a test file and extract functions with their expected outputs
pub fn parse_test_file(content: &str) -> Vec<TestCase> {
    let lines: Vec<&str> = content.lines().collect();
    let mut test_cases = Vec::new();
    let mut i = 0;

    // Parse test command from header
    let mut command = String::new();
    while i < lines.len() {
        let line = lines[i].trim();
        if line.starts_with("test ") {
            command = String::from(line);
            i += 1;
            // Skip blank lines after command
            while i < lines.len() && lines[i].trim().is_empty() {
                i += 1;
            }
            break;
        }
        i += 1;
    }

    // Parse functions and their expected outputs
    while i < lines.len() {
        // Skip blank lines
        if lines[i].trim().is_empty() {
            i += 1;
            continue;
        }

        // Look for function definition
        if lines[i].trim().starts_with("function ") {
            let function_start = i;
            let mut brace_count = 0;
            let mut function_end = i;

            // Find the end of the function (matching braces)
            for j in i..lines.len() {
                let line = lines[j];
                for ch in line.chars() {
                    if ch == '{' {
                        brace_count += 1;
                    } else if ch == '}' {
                        brace_count -= 1;
                        if brace_count == 0 {
                            function_end = j;
                            break;
                        }
                    }
                }
                if brace_count == 0 {
                    break;
                }
            }

            // Extract function text
            let function_text: String = lines[function_start..=function_end]
                .iter()
                .map(|l| String::from(*l))
                .collect::<Vec<_>>()
                .join("\n");

            // Look for expected output in comments after the function
            let mut expected_start = function_end + 1;
            // Skip blank lines
            while expected_start < lines.len() && lines[expected_start].trim().is_empty() {
                expected_start += 1;
            }

            // Check if there are comments starting with ';'
            if expected_start < lines.len() && lines[expected_start].trim().starts_with(';') {
                let mut expected_end = expected_start;
                // Collect all comment lines until we hit a non-comment line or blank line
                while expected_end < lines.len() {
                    let line = lines[expected_end].trim();
                    if line.is_empty() {
                        // Check if next non-empty line is a comment or function
                        let mut next_non_empty = expected_end + 1;
                        while next_non_empty < lines.len()
                            && lines[next_non_empty].trim().is_empty()
                        {
                            next_non_empty += 1;
                        }
                        if next_non_empty >= lines.len()
                            || lines[next_non_empty].trim().starts_with(';')
                            || lines[next_non_empty].trim().starts_with("function ")
                        {
                            expected_end = next_non_empty;
                            break;
                        }
                        break;
                    } else if line.starts_with(';') {
                        expected_end += 1;
                    } else {
                        break;
                    }
                }

                // Extract expected text (strip ';' prefix from comments)
                let expected_text: String = lines[expected_start..expected_end]
                    .iter()
                    .map(|l| {
                        let trimmed = l.trim();
                        if trimmed.starts_with("; ") {
                            String::from(&trimmed[2..])
                        } else if trimmed.starts_with(';') {
                            String::from(&trimmed[1..])
                        } else {
                            String::from(trimmed)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                test_cases.push(TestCase {
                    function_text,
                    expected_text,
                    command: command.clone(),
                });

                i = expected_end;
            } else {
                // No expected output found - for some test types (verifier, domtree),
                // expected output is embedded in annotations, so we still create a test case
                if command == "test verifier" || command == "test domtree" {
                    test_cases.push(TestCase {
                        function_text,
                        expected_text: String::new(),
                        command: command.clone(),
                    });
                }
                i = function_end + 1;
            }
        } else {
            i += 1;
        }
    }

    test_cases
}

/// Normalize IR text for comparison
pub fn normalize_ir(ir: &str) -> Vec<String> {
    ir.lines()
        .map(|l| String::from(l.trim()))
        .filter(|l| !l.is_empty())
        .collect()
}

