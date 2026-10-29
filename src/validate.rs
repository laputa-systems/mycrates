use crate::error::{Error, Result};
use crate::pins::Pins;
use crate::store::Store;
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

pub fn validate_project(
    state_dir: &Path,
    pins_path: &Path,
    mirror_index_url: &str,
) -> Result<ValidationResult> {
    let store = Store::new(state_dir.to_path_buf());
    let content = fs::read_to_string(pins_path)?;
    let pins: Pins = toml::from_str(&content)
        .map_err(|e| Error::Input(format!("failed to parse pins {}: {e}", pins_path.display())))?;

    let project_name = pins
        .project
        .clone()
        .unwrap_or_else(|| pins_path.file_stem().unwrap().to_string_lossy().to_string());

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
    }

    let is_partial = !pins.unsupported.is_empty();

    if let Some(ref lock_str) = pins.lock_content {
        let val_tmp = state_dir.join(format!(".validate_tmp_{}", std::process::id()));
        let _ = fs::remove_dir_all(&val_tmp);
        fs::create_dir_all(&val_tmp)?;
        let test_proj = val_tmp.join("proj");
        fs::create_dir_all(test_proj.join("src"))?;
        fs::write(test_proj.join("src/lib.rs"), "")?;

        let mut deps_str = String::new();
        for pin in &pins.registry {
            deps_str.push_str(&format!("{} = \"={}\"\n", pin.name, pin.version));
        }
        let cargo_toml = format!(
            "[package]\nname = \"validation-probe\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n{}\n",
            deps_str
        );
        fs::write(test_proj.join("Cargo.toml"), cargo_toml)?;
        fs::write(test_proj.join("Cargo.lock"), lock_str)?;

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
        cmd.env("NO_PROXY", "127.0.0.1,localhost");
        cmd.args(["metadata", "--locked", "--format-version", "1"]);

        let out = cmd.output().map_err(|e| Error::Operation(format!("failed to run cargo metadata: {e}")))?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let _ = fs::remove_dir_all(&val_tmp);
            return Err(Error::Operation(format!("cargo metadata --locked failed: {stderr}")));
        }

        let mut cmd_fetch = Command::new("cargo");
        cmd_fetch.current_dir(&test_proj);
        cmd_fetch.env("CARGO_HOME", &cargo_home);
        cmd_fetch.env("CARGO_TERM_COLOR", "never");
        cmd_fetch.env("CARGO_HTTP_PROXY", "http://127.0.0.1:1");
        cmd_fetch.env("NO_PROXY", "127.0.0.1,localhost");
        cmd_fetch.args(["fetch", "--locked"]);

        let out_fetch = cmd_fetch
            .output()
            .map_err(|e| Error::Operation(format!("failed to run cargo fetch: {e}")))?;
        if !out_fetch.status.success() {
            let stderr = String::from_utf8_lossy(&out_fetch.stderr);
            let _ = fs::remove_dir_all(&val_tmp);
            return Err(Error::Operation(format!("cargo fetch --locked failed: {stderr}")));
        }

        let lock_after = fs::read_to_string(test_proj.join("Cargo.lock"))?;
        let _ = fs::remove_dir_all(&val_tmp);
        if &lock_after != lock_str {
            return Err(Error::Integrity("Cargo.lock modified during verification".to_string()));
        }
    }

    Ok(ValidationResult {
        project: project_name,
        registry_count: pins.registry.len(),
        unsupported_count: pins.unsupported.len(),
        is_partial,
    })
}
