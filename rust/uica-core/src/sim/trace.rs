//! Plain-text event trace format, wire-compatible with Python
//! `generateEventTrace` in `uiCA.py`.
//!
//! One event per line:
//!   C<cycle> <EV> instr=<instrID> rnd=<rnd> lam=<lam> fused=<fUop> uop=<uop>
//!         [port=<port>] [source=<source>]
//!
//! EV alphabet:
//!   P predecoded          (per instrI; lam/fused/uop = -1)
//!   X removedFromIQ       (per instrI)
//!   Q addedToIDQ          (per lamUop; source tag)
//!   I issued              (per fUop; addedToRB / removedFromIDQ)
//!   r readyForDispatch    (per uop)
//!   D dispatched          (per uop; port)
//!   E executed            (per uop)
//!   R retired             (per fUop)

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    Predecoded,
    RemovedFromIq,
    AddedToIdq,
    Issued,
    ReadyForDispatch,
    Dispatched,
    Executed,
    Retired,
}

impl EventKind {
    pub fn tag(self) -> char {
        match self {
            EventKind::Predecoded => 'P',
            EventKind::RemovedFromIq => 'X',
            EventKind::AddedToIdq => 'Q',
            EventKind::Issued => 'I',
            EventKind::ReadyForDispatch => 'r',
            EventKind::Dispatched => 'D',
            EventKind::Executed => 'E',
            EventKind::Retired => 'R',
        }
    }

    /// Secondary sort key. Matches the `order` column in the Python emitter
    /// so traces can be diffed line-by-line without a tiebreaker fetch.
    pub fn order(self) -> u8 {
        match self {
            EventKind::Predecoded => 0,
            EventKind::RemovedFromIq => 1,
            EventKind::AddedToIdq => 2,
            EventKind::Issued => 3,
            EventKind::ReadyForDispatch => 4,
            EventKind::Dispatched => 5,
            EventKind::Executed => 6,
            EventKind::Retired => 7,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TraceEvent {
    pub cycle: u32,
    pub kind: EventKind,
    pub instr_id: i64,
    pub rnd: i64,
    pub lam: i64,
    pub fused: i64,
    pub uop: i64,
    pub port: Option<String>,
    pub source: Option<String>,
}

#[derive(Clone, Debug)]
pub struct TraceWriter {
    events: Vec<TraceEvent>,
}

impl Default for TraceWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl TraceWriter {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    pub fn push(&mut self, event: TraceEvent) {
        self.events.push(event);
    }

    pub fn finish_to_path(mut self, path: &Path) -> io::Result<()> {
        self.events.sort_by(|a, b| {
            a.cycle
                .cmp(&b.cycle)
                .then_with(|| a.kind.order().cmp(&b.kind.order()))
                .then_with(|| a.instr_id.cmp(&b.instr_id))
                .then_with(|| a.rnd.cmp(&b.rnd))
                .then_with(|| a.lam.cmp(&b.lam))
                .then_with(|| a.fused.cmp(&b.fused))
                .then_with(|| a.uop.cmp(&b.uop))
        });

        let file = File::create(path)?;
        let mut w = BufWriter::new(file);
        for ev in &self.events {
            write!(
                w,
                "C{} {} instr={} rnd={} lam={} fused={} uop={}",
                ev.cycle,
                ev.kind.tag(),
                ev.instr_id,
                ev.rnd,
                ev.lam,
                ev.fused,
                ev.uop,
            )?;
            if let Some(port) = &ev.port {
                write!(w, " port={port}")?;
            }
            if let Some(source) = &ev.source {
                write!(w, " source={source}")?;
            }
            writeln!(w)?;
        }
        w.flush()?;
        Ok(())
    }
}
