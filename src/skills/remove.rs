use anyhow::Result;

use crate::config::Config;

pub fn remove_skill(_config: &Config, _name: Option<&str>) -> Result<()> {
    println!("(remove not yet implemented)");
    Ok(())
}
