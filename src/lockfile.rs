use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Deserialize)]
pub struct Lockfile {
    pub version: Option<i32>,
    #[serde(default)]
    pub package: Vec<LockPackage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LockPackage {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub checksum: Option<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum PkgKind {
    CratesIo,
    AlternateRegistry { url: String },
    Git,
    Path,
    Workspace,
    Unsupported,
}

#[derive(Debug, Clone)]
pub struct Classified {
    pub pkg: LockPackage,
    pub kind: PkgKind,
}

pub fn parse_lockfile(s: &str) -> Result<Lockfile> {
    let v: toml::Value = toml::from_str(s).map_err(|e| Error::Input(e.to_string()))?;
    let version = v.get("version").and_then(|x| x.as_integer());
    if let Some(ver) = version {
        if ver != 3 && ver != 4 {
            return Err(Error::Input(format!("unknown lock version {}", ver)));
        }
    }
    let lock: Lockfile = toml::from_str(s).map_err(|e| Error::Input(e.to_string()))?;
    if let Some(ver) = lock.version {
        if ver != 3 && ver != 4 {
            return Err(Error::Input(format!("unknown lock version {}", ver)));
        }
    }
    Ok(lock)
}

pub fn classify(pkg: &LockPackage) -> Classified {
    match &pkg.source {
        None => Classified { pkg: pkg.clone(), kind: PkgKind::Workspace },
        Some(src) => {
            if src.starts_with("git+") {
                Classified { pkg: pkg.clone(), kind: PkgKind::Git }
            } else if src.starts_with("registry+") {
                // registry+https://... or registry+<name>
                if src.contains("index.crates.io") || src == "registry+https://github.com/rust-lang/crates.io-index" {
                    // crates.io
                    if pkg.checksum.is_none() {
                        // will be error per rules
                    }
                    Classified { pkg: pkg.clone(), kind: PkgKind::CratesIo }
                } else {
                    Classified { pkg: pkg.clone(), kind: PkgKind::AlternateRegistry { url: src.clone() } }
                }
            } else if src.starts_with("path+") || src.contains("(path+") {
                Classified { pkg: pkg.clone(), kind: PkgKind::Path }
            } else {
                Classified { pkg: pkg.clone(), kind: PkgKind::Unsupported }
            }
        }
    }
}
