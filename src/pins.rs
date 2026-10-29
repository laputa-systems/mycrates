use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinEntry {
    pub name: String,
    pub version: String,
    pub checksum: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pins {
    #[serde(default)]
    pub registry: Vec<PinEntry>,
    #[serde(default)]
    pub unsupported: Vec<String>,
}

pub fn write_pins(pins_path: &Path, pins: &Pins) -> Result<()> {
    if let Some(p) = pins_path.parent() {
        fs::create_dir_all(p)?;
    }
    let s = toml::to_string(pins).map_err(|e| crate::error::Error::Operation(e.to_string()))?;
    fs::write(pins_path, s)?;
    Ok(())
}
