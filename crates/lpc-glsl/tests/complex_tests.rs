//! Tests for complex scenarios: factorial, fibonacci, nested control flow
#![cfg(feature = "std")]

mod glsl_test;
use glsl_test::GlslTest;

#[test]
fn test_factorial() {
    let glsl = r#"
        int factorial(int n) {
            if (n <= 1) {
                return 1;
            } else {
                return n * factorial(n - 1);
            }
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "factorial",
        r#"
        function %factorial(i32) -> i32 {
        block0(v0: i32):
            v1 = iconst 1
            v2 = icmp sle v0, v1
            brif v2, block1, block2
        block1:
            v3 = iconst 1
            return v3
        block2:
            v4 = iconst 1
            v5 = isub v0, v4
            call %factorial(v5) -> v6
            v7 = imul v0, v6
            return v7
        }
    "#,
    );
}

#[test]
fn test_fibonacci() {
    let glsl = r#"
        int fibonacci(int n) {
            if (n <= 1) {
                return n;
            } else {
                return fibonacci(n - 1) + fibonacci(n - 2);
            }
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "fibonacci",
        r#"
        function %fibonacci(i32) -> i32 {
        block0(v0: i32):
            v1 = iconst 1
            v2 = icmp sle v0, v1
            brif v2, block1, block2
        block1:
            return v0
        block2:
            v3 = iconst 1
            v4 = isub v0, v3
            call %fibonacci(v4) -> v5
            v6 = iconst 2
            v7 = isub v0, v6
            call %fibonacci(v7) -> v8
            v9 = iadd v5, v8
            return v9
        }
    "#,
    );
}

#[test]
fn test_nested_control_flow() {
    let glsl = r#"
        int main(int x) {
            int result = 0;
            if (x > 0) {
                for (int i = 0; i < x; i = i + 1) {
                    if (i % 2 == 0) {
                        result = result + i;
                    }
                }
            }
            return result;
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "main",
        r#"
        function %main(i32) -> i32 {
        block0(v0: i32):
            v1 = iconst 0
            v2 = iconst 0
            v3 = icmp sgt v0, v2
            brif v3, block1, block2
        block1:
            v4 = iconst 0
            jump block3
        block2:
            jump block10
        block3:
            v5 = icmp slt v4, v0
            brif v5, block4, block6
        block4:
            v6 = iconst 2
            v7 = irem v4, v6
            v8 = iconst 0
            v9 = icmp eq v7, v8
            brif v9, block7, block8
        block5:
            v11 = iconst 1
            v12 = iadd v4, v11
            jump block3
        block6:
            jump block10
        block7:
            v10 = iadd v1, v4
            jump block9
        block8:
            jump block9
        block9:
            jump block5
        block10:
            return v1
        }
    "#,
    );
}

#[test]
fn test_mixed_expressions() {
    let glsl = r#"
        int compute(int a, int b, int c) {
            return (a + b) * (c - a) / (b % c);
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "compute",
        r#"
        function %compute(i32, i32, i32) -> i32 {
        block0(v0: i32, v1: i32, v2: i32):
            v3 = iadd v0, v1
            v4 = isub v2, v0
            v5 = imul v3, v4
            v6 = irem v1, v2
            v7 = idiv v5, v6
            return v7
        }
    "#,
    );
}
