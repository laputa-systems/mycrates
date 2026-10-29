use crate::config::Config;
use crate::error::{Error, Result};
use crate::store::Store;
use crate::validate::validate_project;
use std::env;
use std::fs;
use std::path::PathBuf;

pub fn run(args: &[String]) -> Result<()> {
    let mut state_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(".work/state");
    let mut project: Option<String> = None;
    let mut mirror_url: Option<String> = None;
    let mut cfg_path: Option<PathBuf> = None;

    let mut i = 0;
    while i < args.len() {
        if args[i] == "--state-dir" && i + 1 < args.len() {
            state_dir = PathBuf::from(&args[i + 1]);
            i += 2;
        } else if args[i] == "--project" && i + 1 < args.len() {
            project = Some(args[i + 1].clone());
            i += 2;
        } else if args[i] == "--mirror-url" && i + 1 < args.len() {
            mirror_url = Some(args[i + 1].clone());
            i += 2;
        } else if args[i] == "--config" && i + 1 < args.len() {
            cfg_path = Some(PathBuf::from(&args[i + 1]));
            i += 2;
        } else {
            i += 1;
        }
    }

    let config_file = cfg_path.unwrap_or_else(|| state_dir.join("config.json"));
    let config = Config::load(&config_file).unwrap_or_default();
    let effective_mirror_url = mirror_url.unwrap_or_else(|| {
        format!("{}/current/crates-io/", config.public_base_url.trim_end_matches('/'))
    });

    let store = Store::new(state_dir.clone());
    let pins_dir = store.pins_dir();

    if !pins_dir.exists() {
        println!("validate: no pinned projects found in {}", pins_dir.display());
        return Ok(());
    }

    let mut validated = 0;
    for entry in fs::read_dir(&pins_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("toml") {
            let stem = path.file_stem().unwrap().to_string_lossy().to_string();
            if let Some(ref p) = project {
                if p != &stem {
                    continue;
                }
            }
            let res = validate_project(&state_dir, &path, &effective_mirror_url)?;
            println!(
                "project '{}': {} registry packages verified (partial: {})",
                res.project, res.registry_count, res.is_partial
            );
            validated += 1;
        }
    }

    if validated == 0 {
        if let Some(ref p) = project {
            return Err(Error::Input(format!("project '{}' not found in pins", p)));
        }
    }

    Ok(())
}
