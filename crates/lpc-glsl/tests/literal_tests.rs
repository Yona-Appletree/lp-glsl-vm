//! Tests for literal expressions (int, bool, negative int)

mod glsl_test;
use glsl_test::GlslTest;

#[test]
fn test_int_literal() {
    let glsl = r#"
        int main() {
            return 42;
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "main",
        r#"
        function %main() -> i32 {
        block0:
            v0 = iconst 42
            return v0
        }
    "#,
    );
}

#[test]
fn test_bool_literal_true() {
    let glsl = r#"
        bool main() {
            return true;
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "main",
        r#"
        function %main() -> u32 {
        block0:
            v0 = iconst 1
            return v0
        }
    "#,
    );
}

#[test]
fn test_bool_literal_false() {
    let glsl = r#"
        bool main() {
            return false;
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "main",
        r#"
        function %main() -> u32 {
        block0:
            v0 = iconst 0
            return v0
        }
    "#,
    );
}

#[test]
fn test_negative_int() {
    let glsl = r#"
        int main() {
            return -10;
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "main",
        r#"
        function %main() -> i32 {
        block0:
            v0 = iconst 10
            v2 = iconst 0
            v1 = isub v2, v0
            return v1
        }
    "#,
    );
}
