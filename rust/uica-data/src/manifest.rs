use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{DataPack, MappedUiPack, UiPackError, UIPACK_CHECKSUM_FNV1A64};

pub const DATAPACK_MANIFEST_SCHEMA_VERSION: &str = "uica-datapack-manifest-v2";
pub const DATAPACK_MANIFEST_FILE_NAME: &str = "manifest.json";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataPackManifest {
    pub schema_version: String,
    pub uipack_version: u16,
    pub architectures: std::collections::BTreeMap<String, DataPackManifestArchEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataPackManifestArchEntry {
    pub path: String,
    pub size: u64,
    pub checksum_kind: String,
    pub checksum: String,
    pub record_count: u32,
}

#[derive(Debug)]
pub enum DataPackManifestError {
    Io(std::io::Error),
    Json(serde_json::Error),
    UiPack(UiPackError),
    InvalidManifest(String),
    ArchNotFound(String),
}

impl fmt::Display for DataPackManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "I/O error: {err}"),
            Self::Json(err) => write!(f, "JSON parse error: {err}"),
            Self::UiPack(err) => write!(f, "uipack error: {err}"),
            Self::InvalidManifest(msg) => f.write_str(msg),
            Self::ArchNotFound(arch) => write!(f, "architecture '{arch}' not found in manifest"),
        }
    }
}

impl std::error::Error for DataPackManifestError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::Json(err) => Some(err),
            Self::UiPack(err) => Some(err),
            Self::InvalidManifest(_) | Self::ArchNotFound(_) => None,
        }
    }
}

impl From<std::io::Error> for DataPackManifestError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for DataPackManifestError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl From<UiPackError> for DataPackManifestError {
    fn from(value: UiPackError) -> Self {
        Self::UiPack(value)
    }
}

pub fn load_manifest(path: impl AsRef<Path>) -> Result<DataPackManifest, DataPackManifestError> {
    let bytes = fs::read(path)?;
    let manifest: DataPackManifest = serde_json::from_slice(&bytes)?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

pub fn resolve_manifest_pack_path(
    manifest_path: impl AsRef<Path>,
    arch: &str,
) -> Result<PathBuf, DataPackManifestError> {
    let manifest_path = manifest_path.as_ref();
    let manifest = load_manifest(manifest_path)?;
    let (_, entry) = manifest_arch_entry(&manifest, arch)?;
    resolve_pack_path(manifest_path, &entry.path)
}

pub fn load_manifest_pack(
    manifest_path: impl AsRef<Path>,
    arch: &str,
) -> Result<DataPack, DataPackManifestError> {
    let manifest_path = manifest_path.as_ref();
    let manifest = load_manifest(manifest_path)?;
    let (manifest_arch, entry) = manifest_arch_entry(&manifest, arch)?;
    let pack_path = resolve_pack_path(manifest_path, &entry.path)?;
    let mapped = MappedUiPack::open(&pack_path)?;
    let view = mapped.view()?;
    let header = view.header();

    if manifest.uipack_version != header.version {
        return Err(DataPackManifestError::InvalidManifest(format!(
            "manifest uipack version mismatch for arch '{manifest_arch}': manifest {} != pack {}",
            manifest.uipack_version, header.version
        )));
    }

    if entry.size != header.file_len {
        return Err(DataPackManifestError::InvalidManifest(format!(
            "manifest pack size mismatch for arch '{manifest_arch}': manifest {} != pack {}",
            entry.size, header.file_len
        )));
    }

    if entry.record_count != header.records_count {
        return Err(DataPackManifestError::InvalidManifest(format!(
            "manifest record count mismatch for arch '{manifest_arch}': manifest {} != pack {}",
            entry.record_count, header.records_count
        )));
    }

    let checksum_kind = checksum_kind_name(header.checksum_kind).ok_or_else(|| {
        DataPackManifestError::InvalidManifest(format!(
            "unknown pack checksum kind {} for arch '{manifest_arch}'",
            header.checksum_kind
        ))
    })?;
    if entry.checksum_kind != checksum_kind {
        return Err(DataPackManifestError::InvalidManifest(format!(
            "manifest checksum kind mismatch for arch '{manifest_arch}': manifest '{}' != pack '{}'",
            entry.checksum_kind, checksum_kind
        )));
    }

    let manifest_checksum = parse_checksum(&entry.checksum, manifest_arch)?;
    if manifest_checksum != header.checksum {
        return Err(DataPackManifestError::InvalidManifest(format!(
            "manifest checksum mismatch for arch '{manifest_arch}': manifest {:016x} != pack {:016x}",
            manifest_checksum, header.checksum
        )));
    }

    if !view.arch().eq_ignore_ascii_case(manifest_arch) {
        return Err(DataPackManifestError::InvalidManifest(format!(
            "manifest arch '{manifest_arch}' does not match pack record arch"
        )));
    }

    Ok(view.to_data_pack()?)
}

fn validate_manifest(manifest: &DataPackManifest) -> Result<(), DataPackManifestError> {
    if manifest.schema_version != DATAPACK_MANIFEST_SCHEMA_VERSION {
        return Err(DataPackManifestError::InvalidManifest(format!(
            "unsupported manifest schema version '{}'",
            manifest.schema_version
        )));
    }

    Ok(())
}

fn manifest_arch_entry<'a>(
    manifest: &'a DataPackManifest,
    arch: &str,
) -> Result<(&'a str, &'a DataPackManifestArchEntry), DataPackManifestError> {
    if let Some((name, entry)) = manifest.architectures.get_key_value(arch) {
        return Ok((name.as_str(), entry));
    }

    let normalized = arch.trim().to_ascii_uppercase();
    if let Some((name, entry)) = manifest.architectures.get_key_value(normalized.as_str()) {
        return Ok((name.as_str(), entry));
    }

    manifest
        .architectures
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(&normalized))
        .map(|(name, entry)| (name.as_str(), entry))
        .ok_or(DataPackManifestError::ArchNotFound(normalized))
}

fn resolve_pack_path(
    manifest_path: &Path,
    relative_path: &str,
) -> Result<PathBuf, DataPackManifestError> {
    let relative = Path::new(relative_path);
    if relative.is_absolute() {
        return Err(DataPackManifestError::InvalidManifest(format!(
            "manifest pack path must be relative: '{}'",
            relative_path
        )));
    }

    for component in relative.components() {
        match component {
            Component::Normal(_) => {}
            _ => {
                return Err(DataPackManifestError::InvalidManifest(format!(
                    "manifest pack path contains unsafe component: '{}'",
                    relative_path
                )));
            }
        }
    }

    Ok(manifest_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(relative))
}

fn checksum_kind_name(kind: u16) -> Option<&'static str> {
    match kind {
        UIPACK_CHECKSUM_FNV1A64 => Some("fnv1a64"),
        _ => None,
    }
}

fn parse_checksum(value: &str, arch: &str) -> Result<u64, DataPackManifestError> {
    u64::from_str_radix(value, 16).map_err(|_| {
        DataPackManifestError::InvalidManifest(format!(
            "manifest checksum is not valid hex for arch '{arch}': '{value}'"
        ))
    })
}
