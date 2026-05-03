use std::collections::BTreeMap;

use crate::{DataPack, InstructionRecord};

pub struct DataPackIndex {
    by_arch_and_mnemonic: BTreeMap<(String, String), Vec<InstructionRecord>>,
    empty: Vec<InstructionRecord>,
}

impl DataPackIndex {
    pub fn new(pack: DataPack) -> Self {
        let mut by_arch_and_mnemonic: BTreeMap<(String, String), Vec<InstructionRecord>> =
            BTreeMap::new();

        for record in pack.instructions {
            let arch = record.arch.to_ascii_uppercase();
            let string_mnemonic = normalize_mnemonic(&record.string);
            let iform_mnemonic = normalize_iform_prefix(&record.iform);

            if string_mnemonic == iform_mnemonic {
                by_arch_and_mnemonic
                    .entry((arch, string_mnemonic))
                    .or_default()
                    .push(record);
            } else {
                by_arch_and_mnemonic
                    .entry((arch.clone(), string_mnemonic))
                    .or_default()
                    .push(record.clone());
                by_arch_and_mnemonic
                    .entry((arch, iform_mnemonic))
                    .or_default()
                    .push(record);
            }
        }

        Self {
            by_arch_and_mnemonic,
            empty: Vec::new(),
        }
    }

    pub fn candidates_for(&self, arch: &str, mnemonic: &str) -> &[InstructionRecord] {
        self.by_arch_and_mnemonic
            .get(&(arch.to_ascii_uppercase(), normalize_mnemonic(mnemonic)))
            .map(Vec::as_slice)
            .unwrap_or(self.empty.as_slice())
    }
}

pub(crate) fn normalize_mnemonic(text: &str) -> String {
    let upper = text
        .trim()
        .split(|ch: char| ch.is_whitespace() || ch == '(')
        .find(|part| !part.is_empty())
        .unwrap_or("")
        .to_ascii_uppercase();

    canonical_mnemonic_alias(upper.as_str()).to_string()
}

fn canonical_mnemonic_alias(mnemonic: &str) -> &str {
    if mnemonic.starts_with("VCMP") {
        if mnemonic.ends_with("PS") {
            return "VCMPPS";
        }
        if mnemonic.ends_with("PD") {
            return "VCMPPD";
        }
        if mnemonic.ends_with("SS") {
            return "VCMPSS";
        }
        if mnemonic.ends_with("SD") {
            return "VCMPSD";
        }
    }
    if mnemonic.starts_with("CMP") {
        if mnemonic.ends_with("PS") {
            return "CMPPS";
        }
        if mnemonic.ends_with("PD") {
            return "CMPPD";
        }
        if mnemonic.ends_with("SS") {
            return "CMPSS";
        }
        if mnemonic.ends_with("SD") {
            return "CMPSD";
        }
    }
    match mnemonic {
        "JE" => "JZ",
        "JNE" => "JNZ",
        "CMOVNLE" => "CMOVG",
        "SETZ" => "SETE",
        _ => mnemonic,
    }
}

fn normalize_iform_prefix(iform: &str) -> String {
    normalize_mnemonic(iform.split('_').next().unwrap_or(""))
}
