use crate::error::{Error, Result};
use semver::Version as SemVer;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CrateName(String);

impl CrateName {
    pub fn new(s: &str) -> Result<Self> {
        if s.is_empty() {
            return Err(Error::Input("empty crate name".to_string()));
        }
        let bytes = s.as_bytes();
        if bytes.is_empty() || !bytes[0].is_ascii_alphabetic() {
            return Err(Error::Input("crate name must start with letter".to_string()));
        }
        if bytes.len() > 64 {
            return Err(Error::Input("crate name too long".to_string()));
        }
        for &b in bytes {
            if !(b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
                return Err(Error::Input("invalid crate name character".to_string()));
            }
        }
        Ok(CrateName(s.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn lower(&self) -> String {
        self.0.to_lowercase()
    }

    pub fn norm_key(&self) -> String {
        self.lower().replace('_', "-")
    }
}

impl fmt::Display for CrateName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version(SemVer);

impl Version {
    pub fn new(s: &str) -> Result<Self> {
        let v = SemVer::parse(s).map_err(|e| Error::Input(e.to_string()))?;
        if !v.build.is_empty() {
            return Err(Error::Input("build metadata not allowed".to_string()));
        }
        Ok(Version(v))
    }

    pub fn semver(&self) -> &SemVer {
        &self.0
    }

    pub fn key(&self) -> String {
        format!("{}.{}.{}", self.0.major, self.0.minor, self.0.patch)
    }

    pub fn full(&self) -> String {
        format!("{}{}{}", self.key(), if self.0.pre.is_empty() { String::new() } else { format!("-{}", self.0.pre) }, "")
    }

    pub fn as_str(&self) -> String {
        self.full()
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Dependency {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default)]
    pub version_req: String,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(default)]
    pub optional: bool,
    #[serde(default)]
    pub default_features: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explicit_name_in_toml: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishBody {
    pub name: String,
    pub vers: String,
    pub deps: Vec<Dependency>,
    #[serde(default)]
    pub features: std::collections::BTreeMap<String, Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authors: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keywords: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readme: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readme_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(default)]
    pub badges: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub links: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rust_version: Option<String>,
    #[serde(default)]
    pub checksum: String,
    #[serde(default)]
    pub yanked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexEntry {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub vers: String,
    #[serde(default)]
    pub deps: Vec<IndexDep>,
    #[serde(default)]
    pub cksum: String,
    #[serde(default)]
    pub features: std::collections::BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub yanked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub links: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rust_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub v: Option<u32>,
    #[serde(flatten)]
    pub extra: std::collections::BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexDep {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub req: String,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(default)]
    pub optional: bool,
    #[serde(default)]
    pub default_features: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UpstreamIndexLine {
    pub raw: String,
    pub value: IndexEntry,
}
