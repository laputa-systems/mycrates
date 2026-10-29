#[path = "cli/init.rs"]
pub mod init;
#[path = "cli/plan.rs"]
pub mod plan;
#[path = "cli/import_lock.rs"]
pub mod import_lock;
#[path = "cli/add.rs"]
pub mod add;
#[path = "cli/fetch.rs"]
pub mod fetch;
#[path = "cli/publish.rs"]
pub mod publish;
#[path = "cli/serve_publish.rs"]
pub mod serve_publish;
#[path = "cli/verify.rs"]
pub mod verify;
#[path = "cli/doctor.rs"]
pub mod doctor;
#[path = "cli/validate.rs"]
pub mod validate;

use crate::error::{Error, Result};
use lexopt::prelude::*;

#[derive(Debug)]
pub enum Command {
    Init,
    Plan,
    ImportLock,
    Add,
    Fetch,
    Publish,
    ServePublish,
    Verify,
    Doctor,
    Validate,
}

#[derive(Debug)]
pub struct Args {
    pub command: Command,
    pub args: Vec<String>,
}

fn parse_command(s: &str) -> Option<Command> {
    match s {
        "init" => Some(Command::Init),
        "plan" => Some(Command::Plan),
        "import-lock" => Some(Command::ImportLock),
        "add" => Some(Command::Add),
        "fetch" => Some(Command::Fetch),
        "publish" => Some(Command::Publish),
        "serve-publish" => Some(Command::ServePublish),
        "verify" => Some(Command::Verify),
        "doctor" => Some(Command::Doctor),
        "validate" => Some(Command::Validate),
        _ => None,
    }
}

pub fn parse_args() -> Result<Args> {
    let mut parser = lexopt::Parser::from_env();
    let mut cmd: Option<Command> = None;
    let mut rest: Vec<String> = Vec::new();

    while let Some(arg) = parser.next()? {
        match arg {
            Short('h') | Long("help") => {
                print_help();
                std::process::exit(0);
            }
            Value(v) => {
                let s = v.to_string_lossy();
                if cmd.is_none() {
                    if let Some(c) = parse_command(&s) {
                        cmd = Some(c);
                        continue;
                    }
                }
                rest.push(s.into_owned());
            }
            _ => {}
        }
    }

    let command = cmd.ok_or_else(|| Error::Input("missing command".to_string()))?;
    Ok(Args { command, args: rest })
}

pub fn print_help() {
    println!("mycrates");
    println!();
    println!("Usage: mycrates <COMMAND> [OPTIONS]");
    println!();
    println!("Commands:");
    println!("  init");
    println!("  plan");
    println!("  import-lock");
    println!("  add");
    println!("  fetch");
    println!("  publish");
    println!("  serve-publish");
    println!("  verify");
    println!("  doctor");
    println!("  validate");
    println!();
    println!("Options:");
    println!("  -h, --help  Print help");
}
