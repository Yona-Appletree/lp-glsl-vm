//! Relocation handling

use alloc::string::String;

use crate::backend3::{
    types::InsnIndex,
    vcode::{RelocKind, VCodeReloc},
};

/// Record a relocation in VCode
pub fn record_reloc(
    relocations: &mut alloc::vec::Vec<VCodeReloc>,
    inst_idx: InsnIndex,
    kind: RelocKind,
    target: String,
) {
    relocations.push(VCodeReloc {
        inst_idx,
        kind,
        target,
    });
}
