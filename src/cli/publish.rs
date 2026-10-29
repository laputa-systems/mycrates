use crate::config::Config;
use crate::error::Result;
use crate::publish::Publisher;
use crate::store::Store;
use std::env;
use std::path::PathBuf;

pub fn run(args: &[String]) -> Result<()> {
    let mut state_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(".work/state");
    let mut site_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(".work/site");
    let mut cfg_path: Option<PathBuf> = None;

    let mut i = 0;
    while i < args.len() {
        if args[i] == "--state-dir" && i + 1 < args.len() {
            state_dir = PathBuf::from(&args[i + 1]);
            i += 2;
        } else if args[i] == "--site-dir" && i + 1 < args.len() {
            site_dir = PathBuf::from(&args[i + 1]);
            i += 2;
        } else if args[i] == "--config" && i + 1 < args.len() {
            cfg_path = Some(PathBuf::from(&args[i + 1]));
            i += 2;
        } else {
            i += 1;
        }
    }

    let config_file = cfg_path.unwrap_or_else(|| state_dir.join("config.json"));
    let config = Config::load(&config_file)?;

    let store = Store::new(state_dir);
    let publisher = Publisher::new(&store, site_dir, config.public_base_url);
    let res = publisher.publish()?;

    println!(
        "published generation {} (noop: {})",
        res.generation_id, res.is_noop
    );
    Ok(())
}
