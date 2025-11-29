//! Tests for for loop control flow

mod glsl_test;
use glsl_test::GlslTest;

#[test]
fn test_for_simple() {
    let glsl = r#"
        int main() {
            for (int i = 0; i < 10; i = i + 1) {
                return i;
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
            jump block1
        block1:
            v1 = iconst 10
            v2 = icmp slt v0, v1
            brif v2, block2, block4
        block2:
            return v0
            jump block3
        block3:
            v3 = iconst 1
            v4 = iadd v0, v3
            jump block1
        block4:
            v5 = iconst 0
            return v5
        }
    "#,
    );
}

#[test]
fn test_for_no_init() {
    let glsl = r#"
        int main() {
            int i = 0;
            for (; i < 10; i = i + 1) {
                return i;
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
            jump block1
        block1:
            v1 = iconst 10
            v2 = icmp slt v0, v1
            brif v2, block2, block4
        block2:
            return v0
            jump block3
        block3:
            v3 = iconst 1
            v4 = iadd v0, v3
            jump block1
        block4:
            v5 = iconst 0
            return v5
        }
    "#,
    );
}

#[test]
fn test_for_no_condition() {
    let glsl = r#"
        int main() {
            for (int i = 0; ; i = i + 1) {
                if (i >= 10) {
                    return i;
                }
            }
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "main",
        r#"
        function %main() -> i32 {
        block0:
            v0 = iconst 0
            jump block1
        block1:
            v1 = iconst 1
            jump block2
        block2:
            v2 = iconst 10
            v3 = icmp sge v0, v2
            brif v3, block4, block5
        block3:
            v4 = iconst 1
            v5 = iadd v0, v4
            jump block1
        block4:
            return v0
        block5:
            jump block6
        block6:
            jump block3
        }
    "#,
    );
}

#[test]
fn test_for_no_increment() {
    let glsl = r#"
        int main() {
            for (int i = 0; i < 10; ) {
                i = i + 1;
                return i;
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
            jump block1
        block1:
            v1 = iconst 10
            v2 = icmp slt v0, v1
            brif v2, block2, block4
        block2:
            v3 = iconst 1
            v4 = iadd v0, v3
            return v4
            jump block3
        block3:
            jump block1
        block4:
            v5 = iconst 0
            return v5
        }
    "#,
    );
}

#[test]
fn test_for_nested() {
    let glsl = r#"
        int main() {
            int sum = 0;
            for (int i = 0; i < 5; i = i + 1) {
                for (int j = 0; j < 3; j = j + 1) {
                    sum = sum + 1;
                }
            }
            return sum;
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "main",
        r#"
        function %main() -> i32 {
        block0:
            v0 = iconst 0
            v1 = iconst 0
            jump block1
        block1:
            v2 = iconst 5
            v3 = icmp slt v1, v2
            brif v3, block2, block4
        block2:
            v4 = iconst 0
            jump block5
        block3:
            v11 = iconst 1
            v12 = iadd v1, v11
            jump block1
        block4:
            return v0
        block5:
            v5 = iconst 3
            v6 = icmp slt v4, v5
            brif v6, block6, block8
        block6:
            v7 = iconst 1
            v8 = iadd v0, v7
            jump block7
        block7:
            v9 = iconst 1
            v10 = iadd v4, v9
            jump block5
        block8:
            jump block3
        }
    "#,
    );
}
