//! Tests for expression evaluation (arithmetic, comparison, logical, unary operators)

mod glsl_test;
use glsl_test::GlslTest;

#[test]
fn test_arithmetic_add() {
    let glsl = r#"
        int add(int x, int y) {
            return x + y;
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "add",
        r#"
        function %add(i32, i32) -> i32 {
        block0(v0: i32, v1: i32):
            v2 = iadd v0, v1
            return v2
        }
    "#,
    );
}

#[test]
fn test_arithmetic_sub() {
    let glsl = r#"
        int sub(int x, int y) {
            return x - y;
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "sub",
        r#"
        function %sub(i32, i32) -> i32 {
        block0(v0: i32, v1: i32):
            v2 = isub v0, v1
            return v2
        }
    "#,
    );
}

#[test]
fn test_arithmetic_mul() {
    let glsl = r#"
        int mul(int x, int y) {
            return x * y;
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "mul",
        r#"
        function %mul(i32, i32) -> i32 {
        block0(v0: i32, v1: i32):
            v2 = imul v0, v1
            return v2
        }
    "#,
    );
}

#[test]
fn test_arithmetic_div() {
    let glsl = r#"
        int div(int x, int y) {
            return x / y;
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "div",
        r#"
        function %div(i32, i32) -> i32 {
        block0(v0: i32, v1: i32):
            v2 = idiv v0, v1
            return v2
        }
    "#,
    );
}

#[test]
fn test_arithmetic_mod() {
    let glsl = r#"
        int mod(int x, int y) {
            return x % y;
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "mod",
        r#"
        function %mod(i32, i32) -> i32 {
        block0(v0: i32, v1: i32):
            v2 = irem v0, v1
            return v2
        }
    "#,
    );
}

#[test]
fn test_comparison_eq() {
    let glsl = r#"
        bool eq(int x, int y) {
            return x == y;
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "eq",
        r#"
        function %eq(i32, i32) -> u32 {
        block0(v0: i32, v1: i32):
            v2 = icmp eq v0, v1
            return v2
        }
    "#,
    );
}

#[test]
fn test_comparison_ne() {
    let glsl = r#"
        bool ne(int x, int y) {
            return x != y;
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "ne",
        r#"
        function %ne(i32, i32) -> u32 {
        block0(v0: i32, v1: i32):
            v2 = icmp ne v0, v1
            return v2
        }
    "#,
    );
}

#[test]
fn test_comparison_lt() {
    let glsl = r#"
        bool lt(int x, int y) {
            return x < y;
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "lt",
        r#"
        function %lt(i32, i32) -> u32 {
        block0(v0: i32, v1: i32):
            v2 = icmp slt v0, v1
            return v2
        }
    "#,
    );
}

#[test]
fn test_comparison_le() {
    let glsl = r#"
        bool le(int x, int y) {
            return x <= y;
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "le",
        r#"
        function %le(i32, i32) -> u32 {
        block0(v0: i32, v1: i32):
            v2 = icmp sle v0, v1
            return v2
        }
    "#,
    );
}

#[test]
fn test_comparison_gt() {
    let glsl = r#"
        bool gt(int x, int y) {
            return x > y;
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "gt",
        r#"
        function %gt(i32, i32) -> u32 {
        block0(v0: i32, v1: i32):
            v2 = icmp sgt v0, v1
            return v2
        }
    "#,
    );
}

#[test]
fn test_comparison_ge() {
    let glsl = r#"
        bool ge(int x, int y) {
            return x >= y;
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "ge",
        r#"
        function %ge(i32, i32) -> u32 {
        block0(v0: i32, v1: i32):
            v2 = icmp sge v0, v1
            return v2
        }
    "#,
    );
}

#[test]
fn test_logical_and() {
    let glsl = r#"
        bool and(bool x, bool y) {
            return x && y;
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "and",
        r#"
        function %and(u32, u32) -> u32 {
        block0(v0: u32, v1: u32):
            v2 = iand v0, v1
            return v2
        }
    "#,
    );
}

#[test]
fn test_logical_or() {
    let glsl = r#"
        bool or(bool x, bool y) {
            return x || y;
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "or",
        r#"
        function %or(u32, u32) -> u32 {
        block0(v0: u32, v1: u32):
            v2 = ior v0, v1
            return v2
        }
    "#,
    );
}

#[test]
fn test_unary_minus() {
    let glsl = r#"
        int neg(int x) {
            return -x;
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "neg",
        r#"
        function %neg(i32) -> i32 {
        block0(v0: i32):
            v2 = iconst 0
            v1 = isub v2, v0
            return v1
        }
    "#,
    );
}

#[test]
fn test_unary_not() {
    let glsl = r#"
        bool not(bool x) {
            return !x;
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "not",
        r#"
        function %not(u32) -> u32 {
        block0(v0: u32):
            v2 = iconst 0
            v1 = icmp eq v0, v2
            return v1
        }
    "#,
    );
}
