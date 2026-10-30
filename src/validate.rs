use crate::error::{Error, Result};
use crate::lockfile::{parse_lockfile, LockPackage};
use crate::pins::Pins;
use crate::store::Store;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::Command;

#[derive(Debug)]
pub struct ValidationResult {
    pub project: String,
    pub registry_count: usize,
    pub unsupported_count: usize,
    pub is_partial: bool,
}

pub fn validate_archive(state_dir: &Path, pins_path: &Path) -> Result<ValidationResult> {
    let store = Store::new(state_dir.to_path_buf());
    let content = fs::read_to_string(pins_path)?;
    let pins: Pins = toml::from_str(&content)
        .map_err(|e| Error::Input(format!("failed to parse pins {}: {e}", pins_path.display())))?;

    let project_name = pins
        .project
        .clone()
        .unwrap_or_else(|| pins_path.file_stem().unwrap().to_string_lossy().to_string());

    store.verify()?;

    for pin in &pins.registry {
        let rec_path = store
            .upstream_dir()
            .join(pin.name.to_lowercase())
            .join(format!("{}.json", pin.version));
        let blob_path = store.blobs_dir().join(format!("{}.crate", pin.checksum));

        if !rec_path.exists() || !blob_path.exists() {
            return Err(Error::Integrity(format!(
                "incomplete archive: missing package {} {}",
                pin.name, pin.version
            )));
        }

        let rec_data = fs::read_to_string(&rec_path)?;
        let rec: crate::store::UpstreamRecord = serde_json::from_str(&rec_data).map_err(|e| {
            Error::Integrity(format!("invalid upstream record {}: {e}", rec_path.display()))
        })?;
        if rec.checksum != pin.checksum {
            return Err(Error::Integrity(format!(
                "archive checksum changed for {} {}: pin {} != record {}",
                pin.name, pin.version, pin.checksum, rec.checksum
            )));
        }
        if rec.vers != pin.version || rec.name.to_lowercase() != pin.name.to_lowercase() {
            return Err(Error::Integrity(format!(
                "archive record identity changed for {} {}",
                pin.name, pin.version
            )));
        }
        let entry: crate::model::IndexEntry =
            serde_json::from_str(&rec.raw_index_line).map_err(|e| {
                Error::Integrity(format!("invalid cached index line for {} {}: {e}", pin.name, pin.version))
            })?;
        if entry.cksum != pin.checksum
            || entry.name.to_lowercase() != pin.name.to_lowercase()
            || entry.vers != pin.version
        {
            return Err(Error::Integrity(format!(
                "cached index line mismatch for {} {}",
                pin.name, pin.version
            )));
        }
        if entry.yanked {
            return Err(Error::Integrity(format!(
                "archived package {} {} is yanked upstream",
                pin.name, pin.version
            )));
        }
    }

    if let Some(ref lock_str) = pins.lock_content {
        let parsed = parse_lockfile(lock_str)?;
        let locked: BTreeMap<(String, String), &LockPackage> = parsed
            .package
            .iter()
            .filter(|p| p.checksum.is_some())
            .map(|p| ((p.name.to_lowercase(), p.version.clone()), p))
            .collect();
        for pin in &pins.registry {
            match locked.get(&(pin.name.to_lowercase(), pin.version.clone())) {
                Some(pkg) => {
                    if pkg.checksum.as_deref() != Some(pin.checksum.as_str()) {
                        return Err(Error::Integrity(format!(
                            "pin checksum changed for {} {}",
                            pin.name, pin.version
                        )));
                    }
                }
                None => {
                    return Err(Error::Integrity(format!(
                        "pinned package {} {} missing from lockfile",
                        pin.name, pin.version
                    )));
                }
            }
        }
        for pkg in &parsed.package {
            if let Some(cksum) = &pkg.checksum
                && !pins.registry.iter().any(|p| {
                    p.name.to_lowercase() == pkg.name.to_lowercase()
                        && p.version == pkg.version
                        && &p.checksum == cksum
                })
                && is_crates_io_source(pkg.source.as_deref())
            {
                return Err(Error::Integrity(format!(
                    "lockfile package {} {} not in pinned archive",
                    pkg.name, pkg.version
                )));
            }
        }
    }

    Ok(ValidationResult {
        project: project_name,
        registry_count: pins.registry.len(),
        unsupported_count: pins.unsupported.len(),
        is_partial: !pins.unsupported.is_empty(),
    })
}

fn is_crates_io_source(source: Option<&str>) -> bool {
    match source {
        Some(src) => {
            src.starts_with("registry+")
                && (src.contains("index.crates.io")
                    || src == "registry+https://github.com/rust-lang/crates.io-index")
        }
        None => false,
    }
}

pub fn validate_project(
    state_dir: &Path,
    pins_path: &Path,
    mirror_index_url: &str,
) -> Result<ValidationResult> {
    let archived = validate_archive(state_dir, pins_path)?;
    if archived.registry_count == 0 {
        return Ok(archived);
    }

    let content = fs::read_to_string(pins_path)?;
    let pins: Pins = toml::from_str(&content)
        .map_err(|e| Error::Input(format!("failed to parse pins {}: {e}", pins_path.display())))?;

    let Some(ref lock_str) = pins.lock_content else {
        return Err(Error::Input(
            "validate: pins file has no captured Cargo.lock (re-import the lockfile)".to_string(),
        ));
    };
    let _parsed = parse_lockfile(lock_str)?;

    let val_tmp = state_dir.join(format!(".validate_tmp_{}", std::process::id()));
    let _ = fs::remove_dir_all(&val_tmp);
    fs::create_dir_all(&val_tmp)?;
    let cleanup = ValidateCleanup(val_tmp.clone());
    let test_proj = val_tmp.join("proj");
    reconstruct_project(lock_str, &pins, &test_proj)?;

    let cargo_home = val_tmp.join("cargo_home");
    fs::create_dir_all(&cargo_home)?;
    let cargo_config = format!(
        "[source.crates-io]\nreplace-with = \"mycrates-mirror\"\n\n[source.mycrates-mirror]\nregistry = \"sparse+{}\"\n",
        mirror_index_url
    );
    fs::write(cargo_home.join("config.toml"), cargo_config)?;

    let mut cmd = Command::new("cargo");
    cmd.current_dir(&test_proj);
    cmd.env("CARGO_HOME", &cargo_home);
    cmd.env("CARGO_TERM_COLOR", "never");
    cmd.env("CARGO_HTTP_PROXY", "http://127.0.0.1:1");
    cmd.env("HTTPS_PROXY", "http://127.0.0.1:1");
    cmd.env("NO_PROXY", "127.0.0.1,localhost");
    cmd.args(["metadata", "--locked", "--format-version", "1"]);
    let out = cmd.output().map_err(|e| Error::Operation(format!("failed to run cargo metadata: {e}")))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(Error::Operation(format!("cargo metadata --locked failed: {stderr}")));
    }

    let mut cmd_fetch = Command::new("cargo");
    cmd_fetch.current_dir(&test_proj);
    cmd_fetch.env("CARGO_HOME", &cargo_home);
    cmd_fetch.env("CARGO_TERM_COLOR", "never");
    cmd_fetch.env("CARGO_HTTP_PROXY", "http://127.0.0.1:1");
    cmd_fetch.env("HTTPS_PROXY", "http://127.0.0.1:1");
    cmd_fetch.env("NO_PROXY", "127.0.0.1,localhost");
    cmd_fetch.args(["fetch", "--locked"]);
    let out_fetch = cmd_fetch
        .output()
        .map_err(|e| Error::Operation(format!("failed to run cargo fetch: {e}")))?;
    if !out_fetch.status.success() {
        let stderr = String::from_utf8_lossy(&out_fetch.stderr);
        return Err(Error::Operation(format!("cargo fetch --locked failed: {stderr}")));
    }

    let lock_after = fs::read_to_string(test_proj.join("Cargo.lock"))?;
    drop(cleanup);
    let _ = fs::remove_dir_all(&val_tmp);
    if lock_after != *lock_str {
        return Err(Error::Integrity("Cargo.lock modified during verification".to_string()));
    }

    Ok(archived)
}

struct ValidateCleanup(std::path::PathBuf);

impl Drop for ValidateCleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn reconstruct_project(lock_str: &str, pins: &Pins, dest: &Path) -> Result<()> {
    let parsed = parse_lockfile(lock_str)?;
    let root = root_package_name(&parsed)?;

    let _ = &pins.registry;
    let lock_pkgs: BTreeMap<(String, String), &LockPackage> = parsed
        .package
        .iter()
        .map(|p| ((p.name.to_lowercase(), p.version.clone()), p))
        .collect();

    let graph = dependency_graph(&parsed)?;
    let reachable = reachable_from_root(&root, &graph);

    let mut versions_by_name: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut target_deps: Vec<(String, String, String)> = Vec::new();
    for key in &reachable {
        let Some(pkg) = lock_pkgs.get(key) else {
            continue;
        };
        if pkg.name.to_lowercase() == root.to_lowercase() {
            continue;
        }
        versions_by_name.entry(pkg.name.clone()).or_default().insert(pkg.version.clone());
        let targets = target_cfgs(pkg);
        for t in targets {
            target_deps.push((pkg.name.clone(), pkg.version.clone(), t));
        }
    }

    let has_dupes = versions_by_name.values().any(|v| v.len() > 1);
    let has_targets = !target_deps.is_empty();
    if !has_dupes && !has_targets && can_reconstruct_trivial(&parsed, &root) {
        let mut deps_str = String::new();
        for pin in &pins.registry {
            deps_str.push_str(&format!("{} = \"={}\"\n", pin.name, pin.version));
        }
        fs::create_dir_all(dest.join("src"))?;
        fs::write(dest.join("src/lib.rs"), "")?;
        let cargo_toml = format!(
            "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n{}\n",
            root, deps_str
        );
        fs::write(dest.join("Cargo.toml"), cargo_toml)?;
        fs::write(dest.join("Cargo.lock"), lock_str)?;
        return Ok(());
    }

    if versions_by_name.values().any(|v| v.len() > 1) {
        let mut detail: Vec<String> = versions_by_name
            .iter()
            .filter(|(_, v)| v.len() > 1)
            .map(|(n, v)| format!("{n} ({})", v.iter().cloned().collect::<Vec<_>>().join(", ")))
            .collect();
        detail.sort();
        return Err(Error::Input(format!(
            "validate: lockfile requires duplicate versions ({}) which cannot be represented without the original manifests; re-validate with the project directory",
            detail.join("; ")
        )));
    }
    Err(Error::Input(
        "validate: lockfile shape (target-specific dependencies) cannot be represented without the original manifests; re-validate with the project directory"
            .to_string(),
    ))
}

fn root_package_name(parsed: &crate::lockfile::Lockfile) -> Result<String> {
    let dep_names: BTreeSet<String> = parsed
        .package
        .iter()
        .flat_map(|p| p.dependencies.iter())
        .map(|d| dep_base_name(d).to_lowercase())
        .collect();
    let candidates: Vec<&LockPackage> = parsed
        .package
        .iter()
        .filter(|p| p.source.is_none() && !dep_names.contains(&p.name.to_lowercase()))
        .collect();
    if candidates.len() == 1 {
        return Ok(candidates[0].name.clone());
    }
    Err(Error::Input(
        "validate: cannot determine a single workspace root from Cargo.lock (need the project directory)".to_string(),
    ))
}

fn dep_base_name(dep: &str) -> String {
    dep.split_whitespace().next().unwrap_or(dep).to_string()
}

type DepGraph = BTreeMap<(String, String), Vec<(String, String)>>;

fn dependency_graph(parsed: &crate::lockfile::Lockfile) -> Result<DepGraph> {
    let mut by_name: BTreeMap<String, Vec<&LockPackage>> = BTreeMap::new();
    for p in &parsed.package {
        by_name.entry(p.name.to_lowercase()).or_default().push(p);
    }
    let mut graph = BTreeMap::new();
    for p in &parsed.package {
        let mut edges = Vec::new();
        for dep in &p.dependencies {
            let base = dep_base_name(dep).to_lowercase();
            let entry = (p.name.to_lowercase(), p.version.clone());
            match by_name.get(&base) {
                Some(versions) if versions.len() == 1 => {
                    edges.push((versions[0].name.to_lowercase(), versions[0].version.clone()));
                }
                Some(_) | None => {
                    let _ = entry;
                    return Err(Error::Input(format!(
                        "validate: ambiguous dependency '{dep}' of {} {}; need the project directory",
                        p.name, p.version
                    )));
                }
            }
        }
        graph.insert((p.name.to_lowercase(), p.version.clone()), edges);
    }
    Ok(graph)
}

fn reachable_from_root(root: &str, graph: &DepGraph) -> BTreeSet<(String, String)> {
    let root_key = graph
        .keys()
        .find(|(n, _)| n == &root.to_lowercase())
        .cloned();
    let mut out = BTreeSet::new();
    let mut stack = vec![root_key];
    while let Some(Some(key)) = stack.pop() {
        if !out.insert(key.clone()) {
            continue;
        }
        if let Some(edges) = graph.get(&key) {
            for e in edges {
                stack.push(Some(e.clone()));
            }
        }
    }
    out
}

fn target_cfgs(pkg: &LockPackage) -> Vec<String> {
    pkg.dependencies
        .iter()
        .filter_map(|d| {
            let mut parts = d.split_whitespace();
            let _ = parts.next()?;
            for tok in parts {
                if tok.starts_with("(target") {
                    return Some(d.clone());
                }
            }
            None
        })
        .collect()
}

fn can_reconstruct_trivial(parsed: &crate::lockfile::Lockfile, root: &str) -> bool {
    let root_pkg = parsed.package.iter().find(|p| p.name.to_lowercase() == root.to_lowercase());
    match root_pkg {
        Some(pkg) => {
            if pkg.source.is_some() {
                return false;
            }
            if pkg.dependencies.iter().any(|d| d.contains("(target")) {
                return false;
            }
            true
        }
        None => false,
    }
}

