use std::collections::BTreeMap;

use crate::{DataPack, InstructionRecord};

pub struct DataPackIndex<'a> {
    pack: &'a DataPack,
    by_arch_and_mnemonic: BTreeMap<(String, String), Vec<usize>>,
    empty: Vec<usize>,
}

impl<'a> DataPackIndex<'a> {
    pub fn new(pack: &'a DataPack) -> Self {
        let mut by_arch_and_mnemonic: BTreeMap<(String, String), Vec<usize>> = BTreeMap::new();

        for (index, record) in pack.instructions.iter().enumerate() {
            let arch = record.arch.to_ascii_uppercase();
            let string_mnemonic = normalize_mnemonic(&record.string);
            let iform_mnemonic = normalize_iform_prefix(&record.iform);

            by_arch_and_mnemonic
                .entry((arch.clone(), string_mnemonic.clone()))
                .or_default()
                .push(index);

            if string_mnemonic != iform_mnemonic {
                by_arch_and_mnemonic
                    .entry((arch, iform_mnemonic))
                    .or_default()
                    .push(index);
            }
        }

        Self {
            pack,
            by_arch_and_mnemonic,
            empty: Vec::new(),
        }
    }

    pub fn candidates_for(
        &'a self,
        arch: &str,
        mnemonic: &str,
    ) -> impl Iterator<Item = &'a InstructionRecord> + 'a {
        self.by_arch_and_mnemonic
            .get(&(arch.to_ascii_uppercase(), normalize_mnemonic(mnemonic)))
            .unwrap_or(&self.empty)
            .iter()
            .map(|&index| &self.pack.instructions[index])
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

pub(crate) fn normalize_iform_prefix(iform: &str) -> String {
    normalize_mnemonic(iform.split('_').next().unwrap_or(""))
}
