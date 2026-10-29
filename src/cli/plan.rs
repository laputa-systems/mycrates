use crate::acquire::plan;
use crate::error::Result;
use std::env;
use std::path::PathBuf;

pub fn run(args: &[String]) -> Result<()> {
    let mut state_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(".work/state");

    let mut i = 0;
    while i < args.len() {
        if args[i] == "--state-dir" && i + 1 < args.len() {
            state_dir = PathBuf::from(&args[i + 1]);
            i += 2;
        } else {
            i += 1;
        }
    }

    let items = plan(&state_dir)?;
    if items.is_empty() {
        println!("plan: no packages to fetch");
    } else {
        println!("plan: {} packages to fetch:", items.len());
        for it in items {
            println!("  {}@{} ({})", it.name, it.version, it.checksum);
        }
    }

    Ok(())
}
