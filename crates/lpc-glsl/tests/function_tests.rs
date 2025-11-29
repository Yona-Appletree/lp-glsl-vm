//! Tests for function calls, recursion, and multiple functions

mod glsl_test;
use glsl_test::GlslTest;

#[test]
fn test_function_simple() {
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
fn test_function_no_params() {
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
fn test_function_void() {
    let glsl = r#"
        void main() {
            return;
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "main",
        r#"
        function %main() {
        block0:
            return
        }
    "#,
    );
}

#[test]
fn test_function_multiple_params() {
    let glsl = r#"
        int compute(int a, int b, int c) {
            return a + b + c;
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "compute",
        r#"
        function %compute(i32, i32, i32) -> i32 {
        block0(v0: i32, v1: i32, v2: i32):
            v3 = iadd v0, v1
            v4 = iadd v3, v2
            return v4
        }
    "#,
    );
}

#[test]
fn test_function_call() {
    let glsl = r#"
        int add(int x, int y) {
            return x + y;
        }
        int main() {
            return add(10, 20);
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
            call %add(v0, v1) -> v2
            return v2
        }
    "#,
    );
}

#[test]
fn test_function_recursive() {
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
fn test_multiple_functions() {
    let glsl = r#"
        int add(int x, int y) {
            return x + y;
        }
        int multiply(int x, int y) {
            return x * y;
        }
        int main() {
            return add(2, 3);
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    // Verify all functions compile
    assert!(test.get_function("add").is_some());
    assert!(test.get_function("multiply").is_some());
    assert!(test.get_function("main").is_some());
}

#[test]
fn test_function_call_chain() {
    let glsl = r#"
        int double(int x) {
            return x * 2;
        }
        int quadruple(int x) {
            return double(double(x));
        }
        int main() {
            return quadruple(5);
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "main",
        r#"
        function %main() -> i32 {
        block0:
            v0 = iconst 5
            call %quadruple(v0) -> v1
            return v1
        }
    "#,
    );
}

#[test]
fn test_out_parameter() {
    let glsl = r#"
        void set_value(out int x) {
            x = 42;
        }
        int main() {
            int result;
            set_value(result);
            return result;
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "set_value",
        r#"
        function %set_value(i32) {
        block0(v0: i32):
            v1 = iconst 0
            v2 = iconst 42
            store.i32 v0, v2
            return
        }
    "#,
    );
    test.assert_lpir(
        "main",
        r#"
        function %main() -> i32 {
        block0:
            v0 = iconst 0
            v1 = stackalloc 4
            call %set_value(v1)
            v2 = load.i32 v1
            return v2
        }
    "#,
    );
}

#[test]
fn test_inout_parameter() {
    let glsl = r#"
        void increment(inout int x) {
            x = x + 1;
        }
        int main() {
            int value = 10;
            increment(value);
            return value;
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "increment",
        r#"
        function %increment(i32) {
        block0(v0: i32):
            v1 = load.i32 v0
            v2 = iconst 1
            v3 = iadd v1, v2
            store.i32 v0, v3
            return
        }
    "#,
    );
    test.assert_lpir(
        "main",
        r#"
        function %main() -> i32 {
        block0:
            v0 = iconst 10
            v1 = stackalloc 4
            store.i32 v1, v0
            call %increment(v1)
            v2 = load.i32 v1
            return v2
        }
    "#,
    );
}

#[test]
fn test_swap_inout() {
    let glsl = r#"
        void swap(inout int x, inout int y) {
            int temp = x;
            x = y;
            y = temp;
        }
        int main() {
            int a = 10;
            int b = 20;
            swap(a, b);
            return a; // Should return 20 after swap
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "swap",
        r#"
        function %swap(i32, i32) {
        block0(v0: i32, v1: i32):
            v2 = load.i32 v0
            v3 = load.i32 v1
            store.i32 v0, v3
            store.i32 v1, v2
            return
        }
    "#,
    );
    test.assert_lpir(
        "main",
        r#"
        function %main() -> i32 {
        block0:
            v0 = iconst 10
            v1 = iconst 20
            v2 = stackalloc 4
            store.i32 v2, v0
            v3 = stackalloc 4
            store.i32 v3, v1
            call %swap(v2, v3)
            v4 = load.i32 v2
            v5 = load.i32 v3
            return v4
        }
    "#,
    );
}
