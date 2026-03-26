use std::path::PathBuf;
use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::config::Marketplace;
use crate::util::cache_dir;

/// Returns the cache path for a marketplace: `~/.agents/cache/<name-with-dashes>/`.
pub fn marketplace_cache_path(marketplace: &Marketplace) -> Result<PathBuf> {
    let dir_name = marketplace.name.replace('/', "-");
    Ok(cache_dir()?.join(dir_name))
}

/// Clone or update a marketplace repository in the local cache.
pub fn update_marketplace(marketplace: &Marketplace) -> Result<()> {
    let cache_path = marketplace_cache_path(marketplace)?;

    if cache_path.is_dir() {
        println!("Updating marketplace '{}'...", marketplace.name);
        let status = Command::new("git")
            .args(["pull", "--quiet"])
            .current_dir(&cache_path)
            .status()
            .context("failed to run git pull")?;
        if !status.success() {
            bail!("git pull failed for marketplace '{}'", marketplace.name);
        }
    } else {
        println!("Cloning marketplace '{}'...", marketplace.name);
        std::fs::create_dir_all(cache_path.parent().unwrap_or(&cache_path))?;
        let status = Command::new("git")
            .args([
                "clone",
                "--quiet",
                "--depth",
                "1",
                &marketplace.url,
                &cache_path.to_string_lossy(),
            ])
            .status()
            .context("failed to run git clone")?;
        if !status.success() {
            bail!(
                "git clone failed for marketplace '{}'",
                marketplace.name
            );
        }
    }

    Ok(())
}

/// Update all registered marketplaces.
pub fn update_all_marketplaces(marketplaces: &[Marketplace]) -> Result<()> {
    if marketplaces.is_empty() {
        println!("No marketplaces registered. Add one with: skillr market add <repo>");
        return Ok(());
    }

    for marketplace in marketplaces {
        update_marketplace(marketplace)?;
    }

    println!("All marketplaces updated.");
    Ok(())
}
