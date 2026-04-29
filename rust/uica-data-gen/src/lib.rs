use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use uica_data::{
    encode_uipack, read_uipack_header, DataPack, InstructionRecord, PerfRecord,
    DATAPACK_MANIFEST_FILE_NAME, DATAPACK_SCHEMA_VERSION, UIPACK_CHECKSUM_FNV1A64, UIPACK_VERSION,
};

pub use uica_data::{
    DataPackManifest, DataPackManifestArchEntry, DATAPACK_MANIFEST_SCHEMA_VERSION,
};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const ARCH_DIR_NAME: &str = "arch";

fn validate_arch_name(arch: &str) -> Result<()> {
    if !arch.is_empty()
        && arch
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '+'))
    {
        return Ok(());
    }

    Err(format!("unsafe architecture name '{arch}'").into())
}

pub fn convert_xml_to_pack_dir(xml_path: &Path, out_dir: &Path) -> Result<DataPackManifest> {
    let packs_by_arch = parse_xml_to_packs(xml_path)?;
    for arch in packs_by_arch.keys() {
        validate_arch_name(arch)?;
    }

    fs::create_dir_all(out_dir)?;

    let arch_dir = out_dir.join(ARCH_DIR_NAME);
    if arch_dir.exists() {
        fs::remove_dir_all(&arch_dir)?;
    }
    fs::create_dir_all(&arch_dir)?;

    let manifest_path = out_dir.join(DATAPACK_MANIFEST_FILE_NAME);
    if manifest_path.exists() {
        fs::remove_file(&manifest_path)?;
    }

    let mut architectures = BTreeMap::new();
    for (arch, pack) in packs_by_arch {
        let file_name = format!("{arch}.uipack");
        let relative_path = format!("{ARCH_DIR_NAME}/{file_name}");
        let bytes = encode_uipack(&pack, &arch)?;
        let header = read_uipack_header(&bytes)?;
        let pack_path = arch_dir.join(&file_name);
        fs::write(pack_path, &bytes)?;

        architectures.insert(
            arch,
            DataPackManifestArchEntry {
                path: relative_path,
                size: header.file_len,
                checksum_kind: checksum_kind_name(header.checksum_kind).to_string(),
                checksum: format!("{:016x}", header.checksum),
                record_count: header.records_count,
            },
        );
    }

    let manifest = DataPackManifest {
        schema_version: DATAPACK_MANIFEST_SCHEMA_VERSION.to_string(),
        uipack_version: UIPACK_VERSION,
        architectures,
    };

    fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)?;

    Ok(manifest)
}

pub fn convert_xml_to_pack(xml_path: &Path, out_dir: &Path) -> Result<DataPackManifest> {
    convert_xml_to_pack_dir(xml_path, out_dir)
}

fn parse_xml_to_packs(xml_path: &Path) -> Result<BTreeMap<String, DataPack>> {
    let xml = fs::read_to_string(xml_path)?;
    let doc = roxmltree::Document::parse(&xml)?;

    let mut dedup: BTreeMap<(String, String, String), InstructionRecord> = BTreeMap::new();
    for instruction in doc
        .descendants()
        .filter(|node| node.has_tag_name("instruction"))
    {
        let Some(iform) = instruction.attribute("iform") else {
            continue;
        };
        let Some(string) = instruction.attribute("string") else {
            continue;
        };

        for arch in instruction
            .children()
            .filter(|node| node.has_tag_name("architecture"))
        {
            let Some(arch_name) = arch.attribute("name") else {
                continue;
            };
            let Some(measurement) = arch
                .children()
                .find(|node| node.has_tag_name("measurement"))
            else {
                continue;
            };

            let record = InstructionRecord {
                arch: arch_name.to_string(),
                iform: iform.to_string(),
                string: string.to_string(),
                perf: parse_perf(instruction, measurement)?,
            };
            dedup.insert(
                (
                    record.arch.clone(),
                    record.iform.clone(),
                    record.string.clone(),
                ),
                record,
            );
        }
    }

    let mut packs_by_arch: BTreeMap<String, DataPack> = BTreeMap::new();
    for record in dedup.into_values() {
        let arch = record.arch.clone();
        packs_by_arch
            .entry(arch)
            .or_insert_with(|| DataPack {
                schema_version: DATAPACK_SCHEMA_VERSION.to_string(),
                instructions: Vec::new(),
            })
            .instructions
            .push(record);
    }

    Ok(packs_by_arch)
}

fn checksum_kind_name(kind: u16) -> &'static str {
    match kind {
        UIPACK_CHECKSUM_FNV1A64 => "fnv1a64",
        _ => "unknown",
    }
}

fn parse_perf(
    instruction: roxmltree::Node<'_, '_>,
    measurement: roxmltree::Node<'_, '_>,
) -> Result<PerfRecord> {
    use uica_data::{LatencyRecord, OperandRecord};

    // Build idx->name map from instruction operands (stable BTreeMap order = XML order).
    let idx_to_name: std::collections::BTreeMap<String, String> = instruction
        .children()
        .filter(|n| n.has_tag_name("operand"))
        .filter_map(|n| {
            Some((
                n.attribute("idx")?.to_string(),
                n.attribute("name")?.to_string(),
            ))
        })
        .collect();

    // Operands: one per XML <operand> child, in idx order.
    let operands: Vec<OperandRecord> = instruction
        .children()
        .filter(|n| n.has_tag_name("operand"))
        .filter_map(|n| {
            let name = n.attribute("name")?.to_string();
            let r#type = n.attribute("type").unwrap_or("reg").to_string();
            let read = n.attribute("r").is_some();
            let write = n.attribute("w").is_some();
            let is_agen = name.contains("AGEN");
            let mem_operand_role = if r#type == "mem" {
                Some(
                    if is_agen {
                        "agen"
                    } else if read && write {
                        "read_write"
                    } else if write {
                        "write"
                    } else if read {
                        "read"
                    } else {
                        "address"
                    }
                    .to_string(),
                )
            } else {
                None
            };

            Some(OperandRecord {
                name,
                r#type,
                read,
                write,
                implicit: n.attribute("implicit").is_some(),
                flags: parse_flag_groups(n, FlagAccess::Any),
                flags_read: parse_flag_groups(n, FlagAccess::Read),
                flags_write: parse_flag_groups(n, FlagAccess::Write),
                mem_base: n.attribute("base").map(str::to_string),
                mem_index: n.attribute("index").map(str::to_string),
                mem_scale: n.attribute("scale").and_then(|s| s.parse().ok()),
                mem_disp: n.attribute("disp").and_then(|s| s.parse().ok()),
                is_agen,
                mem_operand_role,
            })
        })
        .collect();

    // Latencies: from <latency> children of the <measurement> node.
    let latencies: Vec<LatencyRecord> = measurement
        .children()
        .filter(|n| n.has_tag_name("latency"))
        .filter_map(|lat| {
            let start_name = idx_to_name.get(lat.attribute("start_op")?)?.clone();
            let target_name = idx_to_name.get(lat.attribute("target_op")?)?.clone();
            let cycles = lat
                .attribute("cycles")
                .or_else(|| lat.attribute("min_cycles"))
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(1);
            let cycles_same_reg = lat
                .attribute("cycles_same_reg")
                .and_then(|s| s.parse::<i32>().ok());
            Some(LatencyRecord {
                start_op: start_name,
                target_op: target_name,
                cycles,
                cycles_same_reg,
            })
        })
        .collect();
    let uops = measurement
        .attribute("uops")
        .unwrap_or("0")
        .parse::<i32>()?;
    let retire_slots = measurement
        .attribute("uops_retire_slots")
        .unwrap_or("1")
        .parse::<i32>()?;
    let uops_mite = measurement
        .attribute("uops_MITE")
        .unwrap_or("1")
        .parse::<i32>()?;
    let uops_ms = measurement
        .attribute("uops_MS")
        .unwrap_or("0")
        .parse::<i32>()?;
    let tp = if let Some(value) = measurement.attribute("TP_unrolled") {
        Some(value.parse::<f64>()?)
    } else if let Some(value) = measurement.attribute("TP_loop") {
        Some(value.parse::<f64>()?)
    } else {
        None
    };
    let ports = measurement
        .attribute("ports")
        .map(parse_ports)
        .transpose()?
        .unwrap_or_default();

    let implicit_rsp_change =
        parse_i32_attr(instruction, &["implicitRSPChange", "implicit_rsp_change"]).unwrap_or(0);
    let has_high8_output = parse_bool_attr(instruction, &["high8"]);
    let can_be_used_by_lsd =
        parse_bool_opt_attr(instruction, &["canBeUsedByLSD", "can_be_used_by_lsd"])
            .unwrap_or(uops_ms == 0 && implicit_rsp_change == 0 && !has_high8_output);

    Ok(PerfRecord {
        uops,
        retire_slots,
        uops_mite,
        uops_ms,
        tp,
        ports,
        div_cycles: parse_u32_attr(measurement, &["div_cycles", "divCycles"]).unwrap_or(0),
        may_be_eliminated: parse_bool_attr(instruction, &["mayBeEliminated", "may_be_eliminated"]),
        complex_decoder: parse_bool_attr(instruction, &["complexDecoder", "complex_decoder"]),
        n_available_simple_decoders: parse_u32_attr(
            instruction,
            &["nAvailableSimpleDecoders", "n_available_simple_decoders"],
        )
        .unwrap_or(0),
        lcp_stall: parse_bool_attr(instruction, &["lcpStall", "lcp_stall"]),
        implicit_rsp_change,
        can_be_used_by_lsd,
        cannot_be_in_dsb_due_to_jcc_erratum: parse_bool_attr(
            instruction,
            &[
                "cannotBeInDSBDueToJCCErratum",
                "cannot_be_in_dsb_due_to_jcc_erratum",
            ],
        ),
        no_micro_fusion: parse_bool_attr(instruction, &["noMicroFusion", "no_micro_fusion"]),
        no_macro_fusion: parse_bool_attr(instruction, &["noMacroFusion", "no_macro_fusion"]),
        operands,
        latencies,
    })
}

#[derive(Clone, Copy)]
enum FlagAccess {
    Any,
    Read,
    Write,
}

fn parse_flag_groups(node: roxmltree::Node<'_, '_>, access: FlagAccess) -> Vec<String> {
    let mut groups = Vec::new();
    for attr in node.attributes() {
        let group = match attr.name() {
            "flag_CF" => Some("C"),
            "flag_PF" | "flag_AF" | "flag_ZF" | "flag_SF" | "flag_OF" => Some("SPAZO"),
            _ => None,
        };
        let Some(group) = group else { continue };
        let access_matches = match access {
            FlagAccess::Any => true,
            FlagAccess::Read => attr.value().contains('r'),
            FlagAccess::Write => attr.value().contains('w'),
        };
        if access_matches && !groups.iter().any(|existing| existing == group) {
            groups.push(group.to_string());
        }
    }
    groups
}

fn parse_u32_attr(node: roxmltree::Node<'_, '_>, names: &[&str]) -> Option<u32> {
    names
        .iter()
        .find_map(|name| node.attribute(*name))
        .and_then(|value| value.parse().ok())
}

fn parse_i32_attr(node: roxmltree::Node<'_, '_>, names: &[&str]) -> Option<i32> {
    names
        .iter()
        .find_map(|name| node.attribute(*name))
        .and_then(|value| value.parse().ok())
}

fn parse_bool_attr(node: roxmltree::Node<'_, '_>, names: &[&str]) -> bool {
    parse_bool_opt_attr(node, names).unwrap_or(false)
}

fn parse_bool_opt_attr(node: roxmltree::Node<'_, '_>, names: &[&str]) -> Option<bool> {
    names
        .iter()
        .find_map(|name| node.attribute(*name))
        .map(|value| matches!(value, "1" | "true" | "True" | "yes" | "Y"))
}

fn parse_ports(value: &str) -> Result<BTreeMap<String, i32>> {
    let mut ports = BTreeMap::new();

    for chunk in value.split('+').filter(|chunk| !chunk.trim().is_empty()) {
        let (count_part, port_part) = chunk
            .split_once('*')
            .ok_or_else(|| format!("invalid port chunk '{chunk}'"))?;
        let count = count_part.trim().parse::<i32>()?;
        let port_key = port_part.trim().trim_start_matches('p').to_string();
        if port_key.is_empty() {
            return Err(format!("invalid port key in '{chunk}'").into());
        }
        *ports.entry(port_key).or_insert(0) += count;
    }

    Ok(ports)
}
