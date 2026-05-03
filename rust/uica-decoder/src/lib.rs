use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use object::{Object, ObjectSection};

pub use uica_xed::{decode_raw, DecodedInstruction, DecodedMemAddr};

pub fn extract_text_from_object(path: impl AsRef<Path>) -> Result<Vec<u8>> {
    let path = path.as_ref();
    let bytes =
        fs::read(path).with_context(|| format!("failed to read object file {}", path.display()))?;
    let file = object::File::parse(&*bytes)
        .with_context(|| format!("failed to parse object file {}", path.display()))?;
    let section = file
        .section_by_name(".text")
        .context("object missing .text section")?;
    let data = section
        .uncompressed_data()
        .context("failed to read .text section")?;
    Ok(data.into_owned())
}
