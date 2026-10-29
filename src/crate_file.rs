use crate::error::{Error, Result};
use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use std::io::Read;
use tar::Archive;

#[derive(Debug, Clone)]
pub struct ScanLimits {
    pub max_compressed: u64,
    pub max_expanded: u64,
    pub max_entries: usize,
    pub max_ratio: u64,
}

impl Default for ScanLimits {
    fn default() -> Self {
        Self {
            max_compressed: 1024 * 1024 * 1024,
            max_expanded: 4 * 1024 * 1024 * 1024,
            max_entries: 200_000,
            max_ratio: 1000,
        }
    }
}

#[derive(Debug)]
pub struct ScanResult {
    pub checksum: String,
    pub cargo_toml: String,
}

pub fn scan_crate<R: Read>(mut reader: R, name: &str, version: &str, limits: &ScanLimits) -> Result<ScanResult> {
    let mut tmp = Vec::new();
    std::io::copy(&mut reader, &mut tmp).map_err(|e| Error::Operation(e.to_string()))?;
    if tmp.len() as u64 > limits.max_compressed {
        return Err(Error::Limit("compressed size exceeded".to_string()));
    }
    let mut hasher = Sha256::new();
    hasher.update(&tmp);
    let checksum = format!("{:x}", hasher.finalize_reset());

    let gz = GzDecoder::new(&tmp[..]);
    let mut archive = Archive::new(gz);
    let mut entries_seen: usize = 0;
    let mut expanded: u64 = 0;
    let mut found_cargo_toml: Option<String> = None;
    let prefix = format!("{}-{}/", name, version);

    for entry in archive.entries().map_err(|e| Error::Integrity(e.to_string()))? {
        let mut entry = entry.map_err(|e| Error::Integrity(e.to_string()))?;
        entries_seen += 1;
        if entries_seen > limits.max_entries {
            return Err(Error::Limit("too many entries".to_string()));
        }
        let header = entry.header();
        if header.entry_type().is_symlink() || header.entry_type().is_hard_link() {
            return Err(Error::Integrity("symlink or hardlink not allowed".to_string()));
        }
        let path = entry.path().map_err(|e| Error::Integrity(e.to_string()))?;
        let path_str = path.to_string_lossy();
        if path_str.contains("..") || path_str.starts_with('/') {
            return Err(Error::Integrity("path traversal".to_string()));
        }
        if !path_str.starts_with(&prefix) {
            return Err(Error::Integrity("wrong prefix".to_string()));
        }
        let size = entry.size();
        expanded += size;
        if expanded > limits.max_expanded {
            return Err(Error::Limit("expanded size exceeded".to_string()));
        }
        if limits.max_compressed > 0 && expanded / ((tmp.len() as u64).max(1)) > limits.max_ratio {
            return Err(Error::Limit("expansion ratio exceeded".to_string()));
        }
        if path_str == format!("{}Cargo.toml", prefix) {
            let mut s = String::new();
            entry.read_to_string(&mut s).map_err(|e| Error::Integrity(e.to_string()))?;
            found_cargo_toml = Some(s);
        }
    }

    let cargo_toml = found_cargo_toml.ok_or_else(|| Error::Integrity("missing Cargo.toml".to_string()))?;
    let parsed: toml::Value = toml::from_str(&cargo_toml).map_err(|e| Error::Integrity(e.to_string()))?;
    if let Some(pkg) = parsed.get("package") {
        let pkg_name = pkg.get("name").and_then(|v| v.as_str());
        let pkg_ver = pkg.get("version").and_then(|v| v.as_str());
        if pkg_name != Some(name) || pkg_ver != Some(version) {
            return Err(Error::Integrity("name/version mismatch".to_string()));
        }
    } else {
        return Err(Error::Integrity("missing package in Cargo.toml".to_string()));
    }

    Ok(ScanResult { checksum, cargo_toml })
}
