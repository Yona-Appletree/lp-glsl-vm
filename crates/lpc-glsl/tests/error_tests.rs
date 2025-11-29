//! Tests for error cases: type errors, parse errors, undefined variables/functions, etc.

mod glsl_test;
use lpc_glsl::{parse_glsl, GlslError, TypeChecker};

#[test]
fn test_undefined_variable() {
    let glsl = r#"
        int main() {
            return x;
        }
    "#;

    let functions = parse_glsl(glsl).unwrap();
    let mut checker = TypeChecker::new();
    checker.register_functions(&functions).unwrap();

    let result = checker.type_check_function_body(&functions[0].definition);
    assert!(result.is_err());
    if let Err(GlslError::TypeError(msg)) = result {
        assert!(msg.contains("Undefined variable"));
    } else {
        panic!("Expected TypeError for undefined variable");
    }
}

#[test]
fn test_undefined_function() {
    let glsl = r#"
        int main() {
            return unknown_func(10);
        }
    "#;

    let functions = parse_glsl(glsl).unwrap();
    let mut checker = TypeChecker::new();
    checker.register_functions(&functions).unwrap();

    let result = checker.type_check_function_body(&functions[0].definition);
    assert!(result.is_err());
    if let Err(GlslError::TypeError(msg)) = result {
        assert!(msg.contains("Undefined function"));
    } else {
        panic!("Expected TypeError for undefined function");
    }
}

#[test]
fn test_function_call_wrong_arg_count() {
    let glsl = r#"
        int add(int x, int y) {
            return x + y;
        }
        int main() {
            return add(10);
        }
    "#;

    let functions = parse_glsl(glsl).unwrap();
    let mut checker = TypeChecker::new();
    checker.register_functions(&functions).unwrap();

    let result = checker.type_check_function_body(&functions[1].definition);
    assert!(result.is_err());
    if let Err(GlslError::TypeError(msg)) = result {
        assert!(msg.contains("expects 2 arguments"));
    } else {
        panic!("Expected TypeError for wrong argument count");
    }
}

#[test]
fn test_function_call_wrong_arg_type() {
    let glsl = r#"
        int add(int x, int y) {
            return x + y;
        }
        int main() {
            return add(10, true);
        }
    "#;

    let functions = parse_glsl(glsl).unwrap();
    let mut checker = TypeChecker::new();
    checker.register_functions(&functions).unwrap();

    let result = checker.type_check_function_body(&functions[1].definition);
    assert!(result.is_err());
    if let Err(GlslError::TypeError(msg)) = result {
        assert!(msg.contains("Type mismatch"));
    } else {
        panic!("Expected TypeError for wrong argument type");
    }
}

#[test]
fn test_return_type_mismatch() {
    let glsl = r#"
        int main() {
            return true;
        }
    "#;

    let functions = parse_glsl(glsl).unwrap();
    let mut checker = TypeChecker::new();
    checker.register_functions(&functions).unwrap();

    let result = checker.type_check_function_body(&functions[0].definition);
    assert!(result.is_err());
    if let Err(GlslError::TypeError(msg)) = result {
        assert!(msg.contains("Return type mismatch"));
    } else {
        panic!("Expected TypeError for return type mismatch");
    }
}

#[test]
fn test_void_function_returns_value() {
    let glsl = r#"
        void main() {
            return 42;
        }
    "#;

    let functions = parse_glsl(glsl).unwrap();
    let mut checker = TypeChecker::new();
    checker.register_functions(&functions).unwrap();

    let result = checker.type_check_function_body(&functions[0].definition);
    assert!(result.is_err());
    if let Err(GlslError::TypeError(msg)) = result {
        assert!(msg.contains("Void function cannot return a value"));
    } else {
        panic!("Expected TypeError for void function returning value");
    }
}

#[test]
fn test_non_void_function_no_return() {
    let glsl = r#"
        int main() {
            int x = 10;
        }
    "#;

    let functions = parse_glsl(glsl).unwrap();
    let mut checker = TypeChecker::new();
    checker.register_functions(&functions).unwrap();

    let result = checker.type_check_function_body(&functions[0].definition);
    assert!(result.is_err());
    if let Err(GlslError::TypeError(msg)) = result {
        assert!(msg.contains("must return a value"));
    } else {
        panic!("Expected TypeError for non-void function without return");
    }
}

#[test]
fn test_assignment_type_mismatch() {
    let glsl = r#"
        int main() {
            int x = 10;
            x = true;
            return x;
        }
    "#;

    let functions = parse_glsl(glsl).unwrap();
    let mut checker = TypeChecker::new();
    checker.register_functions(&functions).unwrap();

    let result = checker.type_check_function_body(&functions[0].definition);
    assert!(result.is_err());
    if let Err(GlslError::TypeError(msg)) = result {
        assert!(msg.contains("Assignment type mismatch"));
    } else {
        panic!("Expected TypeError for assignment type mismatch");
    }
}

#[test]
fn test_assignment_to_expression() {
    let glsl = r#"
        int main() {
            int x = 10;
            (x + 1) = 20;
            return x;
        }
    "#;

    let functions = parse_glsl(glsl).unwrap();
    let mut checker = TypeChecker::new();
    checker.register_functions(&functions).unwrap();

    let result = checker.type_check_function_body(&functions[0].definition);
    assert!(result.is_err());
    if let Err(GlslError::TypeError(msg)) = result {
        assert!(msg.contains("Assignment can only be to a variable"));
    } else {
        panic!("Expected TypeError for assignment to expression");
    }
}

#[test]
fn test_binary_op_type_mismatch() {
    let glsl = r#"
        int main() {
            return 10 + true;
        }
    "#;

    let functions = parse_glsl(glsl).unwrap();
    let mut checker = TypeChecker::new();
    checker.register_functions(&functions).unwrap();

    let result = checker.type_check_function_body(&functions[0].definition);
    assert!(result.is_err());
    if let Err(GlslError::TypeError(msg)) = result {
        assert!(msg.contains("Arithmetic operator requires int"));
    } else {
        panic!("Expected TypeError for binary op type mismatch");
    }
}

#[test]
fn test_comparison_type_mismatch() {
    let glsl = r#"
        bool main() {
            return 10 < true;
        }
    "#;

    let functions = parse_glsl(glsl).unwrap();
    let mut checker = TypeChecker::new();
    checker.register_functions(&functions).unwrap();

    let result = checker.type_check_function_body(&functions[0].definition);
    assert!(result.is_err());
    if let Err(GlslError::TypeError(msg)) = result {
        assert!(msg.contains("Comparison operator requires matching types"));
    } else {
        panic!("Expected TypeError for comparison type mismatch");
    }
}

#[test]
fn test_logical_op_type_mismatch() {
    let glsl = r#"
        bool main() {
            return true && 10;
        }
    "#;

    let functions = parse_glsl(glsl).unwrap();
    let mut checker = TypeChecker::new();
    checker.register_functions(&functions).unwrap();

    let result = checker.type_check_function_body(&functions[0].definition);
    assert!(result.is_err());
    if let Err(GlslError::TypeError(msg)) = result {
        assert!(msg.contains("Logical operator requires bool"));
    } else {
        panic!("Expected TypeError for logical op type mismatch");
    }
}

#[test]
fn test_unary_op_type_mismatch() {
    let glsl = r#"
        int main() {
            return -true;
        }
    "#;

    let functions = parse_glsl(glsl).unwrap();
    let mut checker = TypeChecker::new();
    checker.register_functions(&functions).unwrap();

    let result = checker.type_check_function_body(&functions[0].definition);
    assert!(result.is_err());
    if let Err(GlslError::TypeError(msg)) = result {
        assert!(msg.contains("Unary minus requires int"));
    } else {
        panic!("Expected TypeError for unary op type mismatch");
    }
}

#[test]
fn test_if_condition_not_bool() {
    let glsl = r#"
        int main() {
            if (10) {
                return 1;
            }
            return 0;
        }
    "#;

    let functions = parse_glsl(glsl).unwrap();
    let mut checker = TypeChecker::new();
    checker.register_functions(&functions).unwrap();

    let result = checker.type_check_function_body(&functions[0].definition);
    assert!(result.is_err());
    if let Err(GlslError::TypeError(msg)) = result {
        assert!(msg.contains("If condition must be bool"));
    } else {
        panic!("Expected TypeError for if condition not bool");
    }
}

#[test]
fn test_while_condition_not_bool() {
    let glsl = r#"
        int main() {
            while (10) {
                return 1;
            }
            return 0;
        }
    "#;

    let functions = parse_glsl(glsl).unwrap();
    let mut checker = TypeChecker::new();
    checker.register_functions(&functions).unwrap();

    let result = checker.type_check_function_body(&functions[0].definition);
    assert!(result.is_err());
    if let Err(GlslError::TypeError(msg)) = result {
        assert!(msg.contains("While condition must be bool"));
    } else {
        panic!("Expected TypeError for while condition not bool");
    }
}

#[test]
fn test_for_condition_not_bool() {
    let glsl = r#"
        int main() {
            for (int i = 0; i; i = i + 1) {
                return i;
            }
            return 0;
        }
    "#;

    let functions = parse_glsl(glsl).unwrap();
    let mut checker = TypeChecker::new();
    checker.register_functions(&functions).unwrap();

    let result = checker.type_check_function_body(&functions[0].definition);
    assert!(result.is_err());
    if let Err(GlslError::TypeError(msg)) = result {
        assert!(msg.contains("For condition must be bool"));
    } else {
        panic!("Expected TypeError for for condition not bool");
    }
}

#[test]
fn test_variable_type_mismatch() {
    let glsl = r#"
        int main() {
            int x = true;
            return x;
        }
    "#;

    let functions = parse_glsl(glsl).unwrap();
    let mut checker = TypeChecker::new();
    checker.register_functions(&functions).unwrap();

    let result = checker.type_check_function_body(&functions[0].definition);
    assert!(result.is_err());
    if let Err(GlslError::TypeError(msg)) = result {
        assert!(msg.contains("type mismatch"));
    } else {
        panic!("Expected TypeError for variable type mismatch");
    }
}

#[test]
fn test_parse_error() {
    let glsl = r#"
        int main() {
            return 10
        }
    "#;

    let result = parse_glsl(glsl);
    assert!(result.is_err());
    if let Err(GlslError::ParseError(_)) = result {
        // Expected
    } else {
        panic!("Expected ParseError for invalid syntax");
    }
}

#[test]
fn test_void_function_call_as_expression() {
    let glsl = r#"
        void set_value() {
        }
        int main() {
            return set_value();
        }
    "#;

    let functions = parse_glsl(glsl).unwrap();
    let mut checker = TypeChecker::new();
    checker.register_functions(&functions).unwrap();

    let result = checker.type_check_function_body(&functions[1].definition);
    assert!(result.is_err());
    if let Err(GlslError::VoidFunctionCall(_)) = result {
        // Expected
    } else {
        panic!("Expected VoidFunctionCall error");
    }
}
