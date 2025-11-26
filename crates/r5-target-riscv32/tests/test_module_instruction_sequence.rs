#[test]
fn test_module_instruction_sequence() {
    // Test that the exact module generates the expected instruction sequence
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

    use r5_ir::parse_module;
    use r5_target_riscv32::compile_module_to_insts;
    use riscv32_encoder::{disassemble_code, Gpr, Inst as RiscvInst};
    let module = parse_module(ir).expect("Failed to parse module");
    let compiled = compile_module_to_insts(&module).expect("Compilation failed");

    // Print assembly from CompiledModule
    eprintln!("=== Assembly from CompiledModule ===");
    eprintln!("{}", compiled);

    // Compile to bytes and disassemble
    let bytes = compiled.to_bytes().expect("Failed to convert to bytes");
    eprintln!("=== Disassembled from bytes ===");
    eprintln!("{}", disassemble_code(&bytes));

    // Compare instructions directly
    let expected: Vec<riscv32_encoder::Inst> = vec![
        RiscvInst::Lui {
            rd: Gpr::SP,
            imm: 2151612416,
        },
        RiscvInst::Addi {
            rd: Gpr::SP,
            rs1: Gpr::SP,
            imm: -24,
        },
        RiscvInst::Sw {
            rs1: Gpr::SP,
            rs2: Gpr::RA,
            imm: 4,
        },
        RiscvInst::Sw {
            rs1: Gpr::SP,
            rs2: Gpr::A0,
            imm: -8,
        },
        RiscvInst::Jal {
            rd: Gpr::RA,
            imm: 40,
        },
        RiscvInst::Lw {
            rd: Gpr::A0,
            rs1: Gpr::SP,
            imm: -8,
        },
        RiscvInst::Addi {
            rd: Gpr::A7,
            rs1: Gpr::ZERO,
            imm: 0,
        },
        RiscvInst::Ecall,
        RiscvInst::Ebreak,
        RiscvInst::Lw {
            rd: Gpr::RA,
            rs1: Gpr::SP,
            imm: 4,
        },
        RiscvInst::Addi {
            rd: Gpr::SP,
            rs1: Gpr::SP,
            imm: 24,
        },
        RiscvInst::Jalr {
            rd: Gpr::ZERO,
            rs1: Gpr::RA,
            imm: 0,
        },
        RiscvInst::Addi {
            rd: Gpr::A0,
            rs1: Gpr::ZERO,
            imm: 42,
        },
        RiscvInst::Jalr {
            rd: Gpr::ZERO,
            rs1: Gpr::RA,
            imm: 0,
        },
        RiscvInst::Addi {
            rd: Gpr::SP,
            rs1: Gpr::SP,
            imm: -24,
        },
        RiscvInst::Sw {
            rs1: Gpr::SP,
            rs2: Gpr::RA,
            imm: 4,
        },
        RiscvInst::Sw {
            rs1: Gpr::SP,
            rs2: Gpr::A0,
            imm: -8,
        },
        RiscvInst::Jal {
            rd: Gpr::RA,
            imm: 36,
        },
        RiscvInst::Lw {
            rd: Gpr::A0,
            rs1: Gpr::SP,
            imm: -8,
        },
        RiscvInst::Lw {
            rd: Gpr::RA,
            rs1: Gpr::SP,
            imm: 4,
        },
        RiscvInst::Addi {
            rd: Gpr::SP,
            rs1: Gpr::SP,
            imm: 24,
        },
        RiscvInst::Jalr {
            rd: Gpr::ZERO,
            rs1: Gpr::RA,
            imm: 0,
        },
    ];
    assert_eq!(compiled.instructions, expected);
}
