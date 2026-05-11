use std::collections::BTreeMap;

use uica_data::{InstructionRecord, MappedUiPackRuntime, UiPackRecordView};

use crate::matcher::{match_instruction_record_iter, InstrRecordLike, NormalizedInstrRef};

#[derive(Clone, Copy)]
pub(crate) struct InstructionDataSource<'a> {
    runtime: &'a MappedUiPackRuntime,
}

struct RuntimeCandidate<'a> {
    record_index: u32,
    record: UiPackRecordView<'a>,
    xml_attrs: BTreeMap<String, String>,
}

impl InstrRecordLike for RuntimeCandidate<'_> {
    fn iform(&self) -> &str {
        self.record.iform()
    }

    fn string(&self) -> &str {
        self.record.string()
    }

    fn xml_attrs(&self) -> &BTreeMap<String, String> {
        &self.xml_attrs
    }

    fn imm_zero(&self) -> bool {
        self.record.imm_zero()
    }
}

impl<'a> InstructionDataSource<'a> {
    pub(crate) fn new(runtime: &'a MappedUiPackRuntime) -> Self {
        Self { runtime }
    }

    pub(crate) fn all_ports(&self) -> Result<Vec<String>, String> {
        self.runtime
            .view()
            .map(|view| view.all_ports())
            .map_err(|err| err.to_string())
    }

    pub(crate) fn alu_ports(&self) -> Result<Vec<String>, String> {
        self.runtime
            .view()
            .map(|view| view.alu_ports())
            .map_err(|err| err.to_string())
    }

    pub(crate) fn match_record(
        &self,
        arch: &str,
        mnemonic: &str,
        norm: NormalizedInstrRef<'_>,
    ) -> Result<Option<InstructionRecord>, String> {
        let view = self.runtime.view().map_err(|err| err.to_string())?;
        if !view.arch().eq_ignore_ascii_case(arch) {
            return Ok(None);
        }

        let mut candidates = Vec::new();

        for &record_index in self.runtime.index().record_indices_for_mnemonic(mnemonic) {
            let record = view.record(record_index).map_err(|err| err.to_string())?;
            let xml_attrs = record.xml_attrs().map_err(|err| err.to_string())?;
            candidates.push(RuntimeCandidate {
                record_index,
                record,
                xml_attrs,
            });
        }

        let Some(matched) = match_instruction_record_iter(norm, candidates.iter()) else {
            return Ok(None);
        };

        uica_data::record_view_to_instruction_record(matched.record)
            .map(Some)
            .map_err(|err| format!("uipack record {}: {err}", matched.record_index))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use uica_data::{
        encode_uipack, DataPack as UiPackFixture, InstructionRecord, MappedUiPackRuntime,
        PerfRecord, DATAPACK_SCHEMA_VERSION,
    };

    use super::InstructionDataSource;
    use crate::matcher::NormalizedInstr;

    fn sample_fixture() -> UiPackFixture {
        UiPackFixture {
            schema_version: DATAPACK_SCHEMA_VERSION.to_string(),
            all_ports: vec!["0".to_string(), "1".to_string()],
            alu_ports: vec!["0".to_string(), "1".to_string()],
            instructions: vec![
                record("SKL", "ADD_GPRv_GPRv", "ADD", false, Default::default()),
                record(
                    "SKL",
                    "ADC_GPRv_IMMb_83",
                    "ADC",
                    true,
                    BTreeMap::from([("immzero".to_string(), "1".to_string())]),
                ),
            ],
        }
    }

    fn sample_runtime() -> MappedUiPackRuntime {
        let bytes = encode_uipack(&sample_fixture(), "SKL").unwrap();
        MappedUiPackRuntime::from_bytes(bytes).unwrap()
    }

    fn record(
        arch: &str,
        iform: &str,
        string: &str,
        imm_zero: bool,
        xml_attrs: BTreeMap<String, String>,
    ) -> InstructionRecord {
        InstructionRecord {
            arch: arch.to_string(),
            iform: iform.to_string(),
            string: string.to_string(),
            all_ports: vec!["0".to_string(), "1".to_string()],
            alu_ports: vec!["0".to_string()],
            locked: false,
            xml_attrs,
            imm_zero,
            perf: PerfRecord {
                operands: vec![],
                latencies: vec![],
                uops: 1,
                retire_slots: 1,
                uops_mite: 1,
                uops_ms: 0,
                tp: Some(1.0),
                ports: BTreeMap::from([("0".to_string(), 1)]),
                div_cycles: 0,
                may_be_eliminated: false,
                complex_decoder: false,
                n_available_simple_decoders: 0,
                lcp_stall: false,
                implicit_rsp_change: 0,
                can_be_used_by_lsd: false,
                cannot_be_in_dsb_due_to_jcc_erratum: false,
                no_micro_fusion: false,
                no_macro_fusion: false,
                macro_fusible_with: vec![],
                variants: Default::default(),
            },
        }
    }

    #[test]
    fn runtime_source_matches_record() {
        let runtime = sample_runtime();
        let source = InstructionDataSource::new(&runtime);
        let norm = NormalizedInstr {
            mnemonic: "adc".to_string(),
            iform_signature: "GPRv_IMMb".to_string(),
            immediate: Some(0),
            xml_attrs: BTreeMap::from([("immzero".to_string(), "1".to_string())]),
            ..Default::default()
        };

        let matched = source
            .match_record("skl", "ADC", norm.as_ref())
            .unwrap()
            .unwrap();

        assert_eq!(matched.iform, "ADC_GPRv_IMMb_83");
        assert!(matched.imm_zero);
        assert_eq!(matched.all_ports, ["0", "1"]);
        assert_eq!(matched.alu_ports, ["0", "1"]);
        assert_eq!(source.all_ports().unwrap(), ["0", "1"]);
        assert_eq!(source.alu_ports().unwrap(), ["0", "1"]);
    }

    #[test]
    fn runtime_source_returns_none_for_requested_arch_mismatch() {
        let runtime = sample_runtime();
        let source = InstructionDataSource::new(&runtime);
        let norm = NormalizedInstr {
            mnemonic: "add".to_string(),
            iform_signature: "GPRv_GPRv".to_string(),
            ..Default::default()
        };

        for arch in ["HSW", "ICL"] {
            let matched = source
                .match_record(arch, "ADD", norm.as_ref())
                .expect("arch mismatch should not error");

            assert!(matched.is_none(), "{arch} should not match SKL runtime");
        }
    }
}
