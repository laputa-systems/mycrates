use crate::error::{Error, Result};

pub fn run(_args: &[String]) -> Result<()> {
    Err(Error::Operation("unimplemented".to_string()))
}
