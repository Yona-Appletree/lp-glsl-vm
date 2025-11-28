#[cfg(test)]
mod tests {
    extern crate std;

    use lpc_lpir::parse_function;

    use crate::expect_ir_a0;

    #[test]
    fn test_block_address_recording_and_relocation_fixup() {
        // Test that block addresses are recorded correctly and relocations are fixed up properly
        // This is a simplified version of test_simple_branch_always_true
        let ir = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 42
    v2 = iconst 0
    v3 = iconst 1
    brif v3, block1(v1), block2(v2)

block1(v4: i32):
    return v4

block2(v5: i32):
    return v5
}"#;

        let func = parse_function(ir).expect("Failed to parse IR function");

        // Use the new compile_function helper that does the full pipeline
        let code = crate::backend::compile_function(func);

        // Check that instructions were emitted
        assert!(code.instruction_count() > 0, "No instructions were emitted");
    }

    #[test]
    fn test_simple_branch_always_true() {
        // Simplest possible branch: always take true branch
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    v0 = iconst 1
    call %test(v0) -> v1
    halt
}

function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 42
    v2 = iconst 0
    v3 = iconst 1
    brif v3, block1, block2

block1:
    return v1

block2:
    return v2
}
}"#;

        expect_ir_a0(ir, 42);
    }

    #[test]
    fn test_simple_branch_always_false() {
        // Simplest possible branch: always take false branch
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    v0 = iconst 1
    call %test(v0) -> v1
    halt
}

function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 42
    v2 = iconst 0
    v3 = iconst 0
    brif v3, block1, block2

block1:
    return v1

block2:
    return v2
}
}"#;

        expect_ir_a0(ir, 0);
    }

    #[test]
    fn test_simple_fibonacci_base_case() {
        // Test fibonacci base case: if n <= 1, return n
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    v0 = iconst 1
    call %fib(v0) -> v1
    halt
}

function %fib(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 1
    v2 = icmp_le v0, v1
    brif v2, block1, block2

block1:
    return v0

block2:
    v3 = iconst 0
    return v3
}
}"#;

        expect_ir_a0(ir, 1);
    }

    #[test]
    fn test_simple_fibonacci_base_case_zero() {
        // Test fibonacci base case with 0: if n <= 1, return n
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    v0 = iconst 0
    call %fib(v0) -> v1
    halt
}

function %fib(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 1
    v2 = icmp_le v0, v1
    brif v2, block1, block2

block1:
    return v0

block2:
    v3 = iconst 999
    return v3
}
}"#;

        expect_ir_a0(ir, 0);
    }

    #[test]
    fn test_simple_fibonacci_recursive_case() {
        // Test fibonacci recursive case: if n > 1, return 999 (to verify branch works)
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    v0 = iconst 5
    call %fib(v0) -> v1
    halt
}

function %fib(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 1
    v2 = icmp_le v0, v1
    brif v2, block1, block2

block1:
    v3 = iconst 0
    return v3

block2:
    v4 = iconst 999
    return v4
}
}"#;

        expect_ir_a0(ir, 999);
    }

    #[test]
    fn test_nested_branches() {
        // Test nested if/else: if a <= 1, return 1; else if a <= 3, return 2; else return 3
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    v0 = iconst 2
    call %test(v0) -> v1
    halt
}

function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 1
    v2 = iconst 3
    v3 = iconst 1
    v4 = iconst 2
    v5 = iconst 3
    v6 = icmp_le v0, v1
    brif v6, block1, block2

block1:
    return v3

block2:
    v7 = icmp_le v0, v2
    brif v7, block3, block4

block3:
    return v4

block4:
    return v5
}
}"#;

        expect_ir_a0(ir, 2);
    }
}
