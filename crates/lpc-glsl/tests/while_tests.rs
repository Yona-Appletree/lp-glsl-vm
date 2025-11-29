//! Tests for while loop control flow

mod glsl_test;
use glsl_test::GlslTest;

#[test]
fn test_while_simple() {
    let glsl = r#"
        int main() {
            while (false) {
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
            v0 = iconst 0
            brif v0, block1, block2
        block1:
            v1 = iconst 1
            return v1
            jump block3
        block2:
            v3 = iconst 0
            return v3
        block3:
            v2 = iconst 0
            brif v2, block1, block2
        }
    "#,
    );
}

#[test]
fn test_while_with_variable() {
    let glsl = r#"
        int main() {
            int x = 0;
            while (x < 10) {
                x = x + 1;
            }
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
            v1 = iconst 10
            v2 = icmp slt v0, v1
            brif v2, block1, block2
        block1:
            v3 = iconst 1
            v4 = iadd v0, v3
            jump block3
        block2:
            return v0
        block3:
            v5 = iconst 10
            v6 = icmp slt v0, v5
            brif v6, block1, block2
        }
    "#,
    );
}

#[test]
fn test_while_nested() {
    let glsl = r#"
        int main() {
            int i = 0;
            while (i < 5) {
                int j = 0;
                while (j < 3) {
                    j = j + 1;
                }
                i = i + 1;
            }
            return i;
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "main",
        r#"
        function %main() -> i32 {
        block0:
            v0 = iconst 0
            v1 = iconst 5
            v2 = icmp slt v0, v1
            brif v2, block1, block2
        block1:
            v3 = iconst 0
            v4 = iconst 3
            v5 = icmp slt v3, v4
            brif v5, block3, block4
            jump block6
        block2:
            return v0
        block3:
            v6 = iconst 1
            v7 = iadd v3, v6
            jump block5
        block4:
            v10 = iconst 1
            v11 = iadd v0, v10
        block5:
            v8 = iconst 3
            v9 = icmp slt v3, v8
            brif v9, block3, block4
        block6:
            v12 = iconst 5
            v13 = icmp slt v0, v12
            brif v13, block1, block2
        }
    "#,
    );
}

#[test]
fn test_while_false() {
    let glsl = r#"
        int main() {
            while (false) {
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
            v0 = iconst 0
            brif v0, block1, block2
        block1:
            v1 = iconst 1
            return v1
            jump block3
        block2:
            v3 = iconst 0
            return v3
        block3:
            v2 = iconst 0
            brif v2, block1, block2
        }
    "#,
    );
}
