use crate::acquire::verify;
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

    verify(&state_dir)?;
    println!("verification passed: all blobs and records intact");
    Ok(())
}
