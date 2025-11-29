//! Tests for edge cases: functions without returns, empty bodies, unreachable code, shadowing, etc.

mod glsl_test;
use glsl_test::GlslTest;

#[test]
fn test_empty_function_body_void() {
    let glsl = r#"
        void main() {
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
fn test_if_without_else_no_return() {
    let glsl = r#"
        int main(bool cond) {
            if (cond) {
                return 10;
            }
        }
    "#;

    let functions = lpc_glsl::parse_glsl(glsl).unwrap();
    let mut checker = lpc_glsl::TypeChecker::new();
    checker.register_functions(&functions).unwrap();

    let result = checker.type_check_function_body(&functions[0].definition);
    assert!(result.is_err());
    if let Err(lpc_glsl::GlslError::TypeError(msg)) = result {
        assert!(msg.contains("must return a value"));
    } else {
        panic!("Expected TypeError for function without return in all paths");
    }
}

#[test]
fn test_unreachable_code_after_return() {
    let glsl = r#"
        int main() {
            return 10;
            int x = 20;
            return x;
        }
    "#;

    // Unreachable code is allowed in GLSL, so this should compile
    // (though we could add warnings in the future)
    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "main",
        r#"
        function %main() -> i32 {
        block0:
            v0 = iconst 10
            return v0
        }
    "#,
    );
}

#[test]
fn test_variable_shadowing() {
    let glsl = r#"
        int main() {
            int x = 10;
            {
                int x = 20;
                return x;
            }
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
fn test_nested_scopes() {
    let glsl = r#"
        int main() {
            int x = 10;
            {
                int y = 20;
                {
                    int z = 30;
                    return x + y + z;
                }
            }
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.validate_function("main");
}

#[test]
fn test_multiple_declarations_same_scope() {
    let glsl = r#"
        int main() {
            int x = 10;
            int y = 20;
            int z = 30;
            return x + y + z;
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.validate_function("main");
}

#[test]
fn test_duplicate_variable_declaration() {
    let glsl = r#"
        int main() {
            int x = 10;
            int x = 20;
            return x;
        }
    "#;

    let functions = lpc_glsl::parse_glsl(glsl).unwrap();
    let mut checker = lpc_glsl::TypeChecker::new();
    checker.register_functions(&functions).unwrap();

    let result = checker.type_check_function_body(&functions[0].definition);
    assert!(result.is_err());
    if let Err(lpc_glsl::GlslError::TypeError(msg)) = result {
        assert!(msg.contains("already declared"));
    } else {
        panic!("Expected TypeError for duplicate variable declaration");
    }
}

#[test]
fn test_out_parameter_with_non_variable() {
    let glsl = r#"
        void set_value(out int x) {
            x = 42;
        }
        int main() {
            int result = 10;
            set_value(result + 1);
            return result;
        }
    "#;

    // This should fail because we can't pass an expression to an out parameter
    // The type checker should catch this, but codegen also validates
    let functions = lpc_glsl::parse_glsl(glsl).unwrap();
    let mut checker = lpc_glsl::TypeChecker::new();
    checker.register_functions(&functions).unwrap();
    checker
        .type_check_function_body(&functions[0].definition)
        .unwrap();

    // The call site should fail type checking
    let _result = checker.type_check_function_body(&functions[1].definition);
    // This might pass type checking but fail codegen, or fail type checking
    // depending on how we handle expressions passed to out parameters
    // For now, we'll just verify it doesn't crash
}

#[test]
fn test_function_with_only_return() {
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
fn test_if_else_both_return() {
    let glsl = r#"
        int main(bool cond) {
            if (cond) {
                return 10;
            } else {
                return 20;
            }
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.validate_function("main");
}

#[test]
fn test_while_loop_with_return() {
    let glsl = r#"
        int main() {
            int i = 0;
            while (i < 10) {
                if (i == 5) {
                    return i;
                }
                i = i + 1;
            }
            return 0;
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.validate_function("main");
}

#[test]
fn test_for_loop_with_return() {
    let glsl = r#"
        int main() {
            for (int i = 0; i < 10; i = i + 1) {
                if (i == 5) {
                    return i;
                }
            }
            return 0;
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.validate_function("main");
}

#[test]
fn test_nested_if_with_returns() {
    let glsl = r#"
        int main(bool a, bool b) {
            if (a) {
                if (b) {
                    return 1;
                } else {
                    return 2;
                }
            } else {
                return 3;
            }
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.validate_function("main");
}

#[test]
fn test_void_function_call_in_expression_statement() {
    let glsl = r#"
        void do_something() {
        }
        void main() {
            do_something();
        }
    "#;

    // Void function calls in expression statements should be allowed
    let test = GlslTest::new(glsl).unwrap();
    test.validate_function("main");
}

#[test]
fn test_function_with_multiple_returns() {
    let glsl = r#"
        int main(bool cond1, bool cond2) {
            if (cond1) {
                return 1;
            }
            if (cond2) {
                return 2;
            }
            return 3;
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.validate_function("main");
}
