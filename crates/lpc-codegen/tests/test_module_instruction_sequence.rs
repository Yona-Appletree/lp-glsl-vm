#[test]
fn test_module_instruction_sequence() {
    // Test that the compiled module executes correctly and produces expected results
    let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
call %main() -> v0
syscall 0(v0)
halt
}

function %helper() -> i32 {
block0:
v0 = iconst 42
return v0
}

function %main() -> i32 {
block0:
call %helper() -> v0
return v0
}
}"#;

    use lpc_codegen::{expect_ir_syscall, Gpr};

    // Run until syscall and verify syscall info
    let mut emu = expect_ir_syscall(ir, 0, &[42]);

    // Verify a0 register contains 42
    assert_eq!(
        emu.get_register(Gpr::A0),
        42,
        "Expected a0 register to contain 42 after main() call"
    );

    // Continue execution to EBREAK (halt)
    match emu.step() {
        Ok(lpc_codegen::StepResult::Halted) => {
            // Successfully halted
        }
        Ok(other) => {
            panic!("Expected Halted after syscall, got {:?}", other);
        }
        Err(e) => {
            panic!("Error after syscall: {}", e);
        }
    }
}
