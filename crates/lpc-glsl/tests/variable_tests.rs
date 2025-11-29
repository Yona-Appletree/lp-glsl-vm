//! Tests for variable declarations, assignments, and usage

mod glsl_test;
use glsl_test::GlslTest;

#[test]
fn test_variable_declaration() {
    let glsl = r#"
        int main() {
            int x = 42;
            return x;
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
fn test_variable_declaration_no_init() {
    let glsl = r#"
        int main() {
            int x;
            return x;
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "main",
        r#"
        function %main() -> i32 {
        block0:
            v0 = iconst 0
            return v0
        }
    "#,
    );
}

#[test]
fn test_variable_assignment() {
    let glsl = r#"
        int main() {
            int x = 10;
            x = 20;
            return x;
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "main",
        r#"
        function %main() -> i32 {
        block0:
            v0 = iconst 10
            v1 = iconst 20
            return v1
        }
    "#,
    );
}

#[test]
fn test_variable_usage() {
    let glsl = r#"
        int main() {
            int x = 10;
            int y = 20;
            return x + y;
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "main",
        r#"
        function %main() -> i32 {
        block0:
            v0 = iconst 10
            v1 = iconst 20
            v2 = iadd v0, v1
            return v2
        }
    "#,
    );
}

