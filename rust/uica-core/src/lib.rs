pub mod analytical;
pub mod engine;
pub mod matcher;
pub mod micro_arch;
pub mod report;
pub mod sim;
pub mod x64;

pub use analytical::{compute_issue_limit, compute_port_usage_limit, InstructionPortUsage};
#[cfg(feature = "xed-decoder")]
pub use engine::engine;
pub use engine::{engine_with_decoded, engine_with_decoded_pack};
pub use matcher::{
    match_instruction, match_instruction_record, match_instruction_record_iter,
    match_instruction_record_ref, normalize_mnemonic, CandidateRecord, NormalizedInstr,
    NormalizedInstrRef,
};
pub use micro_arch::{get_micro_arch, supported_real_arches, MicroArchConfig};
pub use x64::get_canonical_reg;
