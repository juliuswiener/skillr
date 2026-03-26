use anyhow::{bail, Context, Result};
use console::Style;

use crate::config::{Config, Marketplace};

use super::cache::update_marketplace;

/// Add a marketplace repository to the config.
pub fn add_marketplace(config: &mut Config, repo: &str) -> Result<()> {
    // Derive name: use the last two path segments (owner/repo) or the whole string
    let name = derive_name(repo);

    // Derive URL: prepend https://github.com/ if not already a URL
    let url = if repo.starts_with("http://") || repo.starts_with("https://") || repo.starts_with("git@") {
        repo.to_string()
    } else {
        format!("https://github.com/{}.git", repo)
    };

    // Check if already registered
    if config.marketplaces.iter().any(|m| m.name == name) {
        bail!("marketplace '{}' is already registered", name);
    }

    let marketplace = Marketplace {
        name: name.clone(),
        url,
    };

    // Clone the marketplace
    update_marketplace(&marketplace)?;

    config.marketplaces.push(marketplace);
    config.save()?;

    println!("Added marketplace '{}'.", name);
    Ok(())
}

/// List all registered marketplaces.
pub fn list_marketplaces(config: &Config) -> Result<()> {
    if config.marketplaces.is_empty() {
        println!("No marketplaces registered. Add one with: skillr market add <repo>");
        return Ok(());
    }

    let dim = Style::new().dim();

    println!("{:<30} {}", "Name", "URL");
    println!("{}", "-".repeat(70));

    for m in &config.marketplaces {
        println!("{:<30} {}", m.name, dim.apply_to(&m.url));
    }

    Ok(())
}

/// Remove a marketplace from the config.
pub fn remove_marketplace(config: &mut Config, name: Option<&str>) -> Result<()> {
    if config.marketplaces.is_empty() {
        println!("No marketplaces registered.");
        return Ok(());
    }

    let target_name = match name {
        Some(n) => n.to_string(),
        None => {
            let names: Vec<String> = config.marketplaces.iter().map(|m| m.name.clone()).collect();
            inquire::Select::new("Select marketplace to remove:", names)
                .prompt()
                .context("selection cancelled")?
        }
    };

    let before = config.marketplaces.len();
    config.marketplaces.retain(|m| m.name != target_name);

    if config.marketplaces.len() == before {
        bail!("marketplace '{}' not found", target_name);
    }

    config.save()?;
    println!("Removed marketplace '{}'.", target_name);
    Ok(())
}

/// Derive a marketplace name from a repo string.
/// For "owner/repo" or a URL like "https://github.com/owner/repo.git", returns "owner/repo".
fn derive_name(repo: &str) -> String {
    let cleaned = repo
        .trim_end_matches('/')
        .trim_end_matches(".git");

    // Try to extract owner/repo from URL
    if let Some(idx) = cleaned.find("github.com/") {
        return cleaned[idx + "github.com/".len()..].to_string();
    }
    if let Some(idx) = cleaned.find("github.com:") {
        return cleaned[idx + "github.com:".len()..].to_string();
    }

    // Already in owner/repo format
    cleaned.to_string()
}
