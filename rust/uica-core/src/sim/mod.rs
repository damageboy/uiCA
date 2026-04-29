//! Structural port of the Python cycle-accurate simulator.
//!
//! Goal: run the same cycle loop as `uiCA.py` — renamer -> reorder buffer ->
//! scheduler -> frontend fill — and emit the same lifecycle events for each
//! uop so we can diff traces against Python directly.
//!
//! Components mirror Python behavior in mainline simulator code; generated
//! traces remain useful diagnostics after functional port work completes.

pub mod cache_blocks;
pub mod decoder;
pub mod dsb;
pub mod frontend;
pub mod ms;
pub mod predecoder;
pub mod renamer;
pub mod reorder_buffer;
pub mod scheduler;
pub mod trace;
pub mod types;
pub mod uop_expand;
pub mod uop_storage;

pub use frontend::FrontEnd;
pub use trace::{EventKind, TraceEvent, TraceWriter};
pub use types::{
    FusedUop, InstrInstance, LaminatedUop, RenamedOperand, StoreBufferEntry, Uop, UopProperties,
    UopSource,
};
