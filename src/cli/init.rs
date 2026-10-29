use crate::config::Config;
use crate::error::Result;
use crate::store::Store;
use std::env;
use std::fs;
use std::path::PathBuf;

pub fn run(args: &[String]) -> Result<()> {
    let mut state_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(".work/state");
    let mut site_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(".work/site");

    let mut i = 0;
    while i < args.len() {
        if args[i] == "--state-dir" && i + 1 < args.len() {
            state_dir = PathBuf::from(&args[i + 1]);
            i += 2;
        } else if args[i] == "--site-dir" && i + 1 < args.len() {
            site_dir = PathBuf::from(&args[i + 1]);
            i += 2;
        } else {
            i += 1;
        }
    }

    let store = Store::new(state_dir.clone());
    store.ensure_dirs()?;
    fs::create_dir_all(&site_dir)?;

    let cfg_path = state_dir.join("config.json");
    if !cfg_path.exists() {
        let cfg = Config::default();
        cfg.save(&cfg_path)?;
    }

    println!("initialized state at {}", state_dir.display());
    Ok(())
}
