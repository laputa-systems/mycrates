use crate::config::Config;
use crate::error::Result;
use std::env;
use std::fs;
use std::path::PathBuf;

pub fn run(_args: &[String]) -> Result<()> {
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let state_dir = cwd.join(".work/state");
    fs::create_dir_all(&state_dir)?;
    let cfg_path = state_dir.join("config.json");
    let _cfg = Config::load(&cfg_path)?;
    let site_dir = cwd.join(".work/site");
    fs::create_dir_all(&site_dir)?;
    println!("initialized");
    Ok(())
}
