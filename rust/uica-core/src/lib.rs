pub mod analytical;
pub mod engine;
mod instruction_data;
pub mod matcher;
pub mod micro_arch;
pub mod report;
pub mod sim;
pub mod x64;

pub use analytical::{compute_issue_limit, compute_port_usage_limit, InstructionPortUsage};
pub use engine::{
    simulate, DecodeErrorPolicy, MissingUipackPolicy, SimulationInput, SimulationOptions,
    SimulationOutput, SimulationRequest, UipackSource,
};
pub use matcher::{
    match_instruction, match_instruction_record, match_instruction_record_iter,
    match_instruction_record_ref, normalize_mnemonic, CandidateRecord, NormalizedInstr,
    NormalizedInstrRef,
};
pub use micro_arch::{get_micro_arch, supported_real_arches, MicroArchConfig};
pub use x64::get_canonical_reg;
