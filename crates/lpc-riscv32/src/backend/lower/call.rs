//! Lower function call instructions.

use super::Lowerer;
use lpc_lpir::Value;

/// Lower CALL: results = callee(args...)
pub fn lower_call(
    lowerer: &mut Lowerer,
    callee: &str,
    args: &[Value],
    results: &[Value],
) {
    // TODO: Implement call lowering with:
    // 1. Argument preparation (registers + stack)
    // 2. Call instruction (jal for direct, jalr for indirect)
    // 3. Return value handling (registers + return area)
    // 4. Multi-return support
    
    // For now, panic
    panic!("CALL lowering not yet implemented: {}", callee);
}
