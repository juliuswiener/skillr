use std::fmt;

use anyhow::{Context, Result};
use walkdir::WalkDir;

use crate::config::Config;
use crate::util::parse_skill_md;

use super::cache::marketplace_cache_path;

/// A skill found in a marketplace, used for display in the select list.
struct MarketSkill {
    name: String,
    description: String,
    marketplace: String,
    /// Path to the directory containing the SKILL.md.
    path: String,
}

impl fmt::Display for MarketSkill {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.description.is_empty() {
            write!(f, "{} ({})", self.name, self.marketplace)
        } else {
            write!(
                f,
                "{} \u{2014} {} ({})",
                self.name, self.description, self.marketplace
            )
        }
    }
}

/// Browse marketplace skills, optionally filtering by query, and install a selected skill.
pub fn browse_marketplace(config: &Config, query: Option<&str>) -> Result<()> {
    if config.marketplaces.is_empty() {
        println!("No marketplaces registered. Add one with: skillr market add <repo>");
        return Ok(());
    }

    let mut skills: Vec<MarketSkill> = Vec::new();

    for marketplace in &config.marketplaces {
        let cache_path = marketplace_cache_path(marketplace)?;
        if !cache_path.is_dir() {
            println!(
                "Marketplace '{}' not cached yet. Run: skillr market update",
                marketplace.name
            );
            continue;
        }

        for entry in WalkDir::new(&cache_path)
            .max_depth(5)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_name() == "SKILL.md" && entry.file_type().is_file() {
                if let Ok(meta) = parse_skill_md(entry.path()) {
                    let skill_dir = entry
                        .path()
                        .parent()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();

                    skills.push(MarketSkill {
                        name: meta.name,
                        description: meta.description,
                        marketplace: marketplace.name.clone(),
                        path: skill_dir,
                    });
                }
            }
        }
    }

    if skills.is_empty() {
        println!("No skills found in any marketplace.");
        return Ok(());
    }

    // Filter by query if provided
    if let Some(q) = query {
        let q_lower = q.to_lowercase();
        skills.retain(|s| {
            s.name.to_lowercase().contains(&q_lower)
                || s.description.to_lowercase().contains(&q_lower)
        });

        if skills.is_empty() {
            println!("No skills matched query '{}'.", q);
            return Ok(());
        }
    }

    let selected = inquire::Select::new("Select a skill to install:", skills)
        .prompt()
        .context("selection cancelled")?;

    crate::skills::install::install_skill(config, Some(&selected.path))?;

    Ok(())
}
