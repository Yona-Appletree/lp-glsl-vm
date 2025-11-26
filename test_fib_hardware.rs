use r5_target_riscv32::expect_ir_a0;

const FIB_SSA_HARDWARE: &str = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    v0 = iconst 10
    call %fib(v0) -> v1
    return v1
}

function %fib(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 1
    v2 = icmp_le v0, v1
    brif v2, block1, block2

block1:
    return v0

block2:
    v3 = iconst 2
    v4 = isub v0, v1
    v5 = isub v0, v3
    call %fib(v4) -> v6
    call %fib(v5) -> v7
    v8 = iadd v6, v7
    return v8
}
}"#;

fn main() {
    println!("Testing FIB_SSA_HARDWARE in emulator...");
    expect_ir_a0(FIB_SSA_HARDWARE, 55);
    println!("Test passed!");
}


