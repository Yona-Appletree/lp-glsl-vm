

#[cfg(test)]
mod tests {
    extern crate std;

    use crate::expect_ir_a0;

    #[test]
    fn test_icmp_le_true() {
        // Test: if n <= 1, return 42, else return 0
        // With n=0, should return 42
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    v0 = iconst 0
    call %test(v0) -> v1
    halt
}

function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 1
    v2 = iconst 42
    v3 = iconst 0
    v4 = icmp_le v0, v1
    brif v4, block1, block2

block1:
    return v2

block2:
    return v3
}
}"#;

        expect_ir_a0(ir, 42);
    }

    #[test]
    fn test_icmp_le_false() {
        // Test: if n <= 1, return 42, else return 0
        // With n=10, should return 0
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    v0 = iconst 10
    call %test(v0) -> v1
    halt
}

function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 1
    v2 = iconst 42
    v3 = iconst 0
    v4 = icmp_le v0, v1
    brif v4, block1, block2

block1:
    return v2

block2:
    return v3
}
}"#;

        expect_ir_a0(ir, 0);
    }

    #[test]
    fn test_icmp_le_equal() {
        // Test: if n <= 1, return 42, else return 0
        // With n=1, should return 42
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
    v1 = iconst 1
    v2 = iconst 42
    v3 = iconst 0
    v4 = icmp_le v0, v1
    brif v4, block1, block2

block1:
    return v2

block2:
    return v3
}
}"#;

        expect_ir_a0(ir, 42);
    }

    #[test]
    fn test_icmp_lt() {
        // Test: if n < 5, return 10, else return 20
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    v0 = iconst 3
    call %test(v0) -> v1
    halt
}

function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 5
    v2 = iconst 10
    v3 = iconst 20
    v4 = icmp_lt v0, v1
    brif v4, block1, block2

block1:
    return v2

block2:
    return v3
}
}"#;

        expect_ir_a0(ir, 10);
    }

    #[test]
    fn test_icmp_gt() {
        // Test: if n > 5, return 10, else return 20
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    v0 = iconst 7
    call %test(v0) -> v1
    halt
}

function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 5
    v2 = iconst 10
    v3 = iconst 20
    v4 = icmp_gt v0, v1
    brif v4, block1, block2

block1:
    return v2

block2:
    return v3
}
}"#;

        expect_ir_a0(ir, 10);
    }

    #[test]
    fn test_icmp_eq() {
        // Test: if n == 5, return 10, else return 20
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    v0 = iconst 5
    call %test(v0) -> v1
    halt
}

function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 5
    v2 = iconst 10
    v3 = iconst 20
    v4 = icmp_eq v0, v1
    brif v4, block1, block2

block1:
    return v2

block2:
    return v3
}
}"#;

        expect_ir_a0(ir, 10);
    }

    #[test]
    fn test_icmp_ne() {
        // Test: if n != 5, return 10, else return 20
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    v0 = iconst 7
    call %test(v0) -> v1
    halt
}

function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 5
    v2 = iconst 10
    v3 = iconst 20
    v4 = icmp_ne v0, v1
    brif v4, block1, block2

block1:
    return v2

block2:
    return v3
}
}"#;

        expect_ir_a0(ir, 10);
    }
}
