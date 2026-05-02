pub mod analytical;
pub mod engine;
pub mod matcher;
pub mod micro_arch;
pub mod report;
pub mod sim;
pub mod x64;

pub use analytical::{compute_issue_limit, compute_port_usage_limit, InstructionPortUsage};
pub use engine::engine;
pub use matcher::{
    match_instruction, match_instruction_record, normalize_mnemonic, CandidateRecord,
    NormalizedInstr,
};
pub use micro_arch::{get_micro_arch, MicroArchConfig};
pub use x64::get_canonical_reg;
