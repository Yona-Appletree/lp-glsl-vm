//! Tests for if/else control flow statements

mod glsl_test;
use glsl_test::GlslTest;

#[test]
fn test_if_simple() {
    let glsl = r#"
        int main() {
            if (true) {
                return 1;
            }
            return 0;
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "main",
        r#"
        function %main() -> i32 {
        block0:
            v0 = iconst 1
            brif v0, block1, block2
        block1:
            v1 = iconst 1
            return v1
        block2:
            jump block3
        block3:
            v2 = iconst 0
            return v2
        }
    "#,
    );
}

#[test]
fn test_if_else() {
    let glsl = r#"
        int main(bool cond) {
            if (cond) {
                return 1;
            } else {
                return 0;
            }
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "main",
        r#"
        function %main(u32) -> i32 {
        block0(v0: u32):
            brif v0, block1, block2
        block1:
            v1 = iconst 1
            return v1
        block2:
            v2 = iconst 0
            return v2
        }
    "#,
    );
}

#[test]
fn test_if_nested() {
    let glsl = r#"
        int main(bool a, bool b) {
            if (a) {
                if (b) {
                    return 2;
                } else {
                    return 1;
                }
            } else {
                return 0;
            }
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "main",
        r#"
        function %main(u32, u32) -> i32 {
        block0(v0: u32, v1: u32):
            brif v0, block1, block2
        block1:
            brif v1, block3, block4
        block2:
            v4 = iconst 0
            return v4
        block3:
            v2 = iconst 2
            return v2
        block4:
            v3 = iconst 1
            return v3
        }
    "#,
    );
}

#[test]
fn test_if_with_return() {
    let glsl = r#"
        int main(bool cond) {
            if (cond) {
                return 10;
            }
            return 20;
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "main",
        r#"
        function %main(u32) -> i32 {
        block0(v0: u32):
            brif v0, block1, block2
        block1:
            v1 = iconst 10
            return v1
        block2:
            jump block3
        block3:
            v2 = iconst 20
            return v2
        }
    "#,
    );
}
