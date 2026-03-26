use std::fmt;
use std::fs;

use anyhow::{bail, Context, Result};
use console::Style;
use inquire::Confirm;
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
    /// Path to the SKILL.md file itself.
    skill_md_path: String,
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
            if entry.file_name() == "SKILL.md" && entry.file_type().is_file()
                && let Ok(meta) = parse_skill_md(entry.path()) {
                    let skill_dir = entry
                        .path()
                        .parent()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();

                    let md_path = entry.path().to_string_lossy().to_string();
                    skills.push(MarketSkill {
                        name: meta.name,
                        description: meta.description,
                        marketplace: marketplace.name.clone(),
                        path: skill_dir,
                        skill_md_path: md_path,
                    });
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

/// Gather all skills from all cached marketplaces.
fn gather_marketplace_skills(config: &Config) -> Result<Vec<MarketSkill>> {
    let mut skills: Vec<MarketSkill> = Vec::new();

    for marketplace in &config.marketplaces {
        let cache_path = marketplace_cache_path(marketplace)?;
        if !cache_path.is_dir() {
            continue;
        }

        for entry in WalkDir::new(&cache_path)
            .max_depth(5)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_name() == "SKILL.md" && entry.file_type().is_file()
                && let Ok(meta) = parse_skill_md(entry.path())
            {
                let skill_dir = entry
                    .path()
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();
                let md_path = entry.path().to_string_lossy().to_string();

                skills.push(MarketSkill {
                    name: meta.name,
                    description: meta.description,
                    marketplace: marketplace.name.clone(),
                    path: skill_dir,
                    skill_md_path: md_path,
                });
            }
        }
    }

    Ok(skills)
}

/// Search marketplace skills by keyword, show full descriptions, and offer to install.
pub fn search_marketplace(config: &Config, query: Option<&str>) -> Result<()> {
    if config.marketplaces.is_empty() {
        bail!("No marketplaces registered. Add one with: skillr market add <repo>");
    }

    let query_str = match query {
        Some(q) => q.to_string(),
        None => inquire::Text::new("Search skills:")
            .prompt()
            .context("input cancelled")?,
    };

    if query_str.trim().is_empty() {
        bail!("Search query cannot be empty");
    }

    let all_skills = gather_marketplace_skills(config)?;

    if all_skills.is_empty() {
        println!("No skills found in any marketplace. Run 'skillr market update' first.");
        return Ok(());
    }

    let q_lower = query_str.to_lowercase();
    let matches: Vec<&MarketSkill> = all_skills
        .iter()
        .filter(|s| {
            s.name.to_lowercase().contains(&q_lower)
                || s.description.to_lowercase().contains(&q_lower)
        })
        .collect();

    if matches.is_empty() {
        println!("No skills matching '{}'.", query_str);
        return Ok(());
    }

    let green = Style::new().green();
    let dim = Style::new().dim();
    let bold = Style::new().bold();

    println!(
        "\nFound {} skill{} matching '{}':\n",
        matches.len(),
        if matches.len() == 1 { "" } else { "s" },
        query_str
    );

    // Display each match with its full description
    for (i, skill) in matches.iter().enumerate() {
        println!(
            "{}. {} {}",
            green.apply_to(i + 1),
            bold.apply_to(&skill.name),
            dim.apply_to(format!("({})", skill.marketplace))
        );

        // Read and display the full SKILL.md content (truncated to a reasonable size)
        if let Ok(content) = fs::read_to_string(&skill.skill_md_path) {
            let body = extract_skill_body(&content);
            if !body.is_empty() {
                // Show first ~20 lines of the body
                let preview: Vec<&str> = body.lines().take(20).collect();
                for line in &preview {
                    println!("   {}", dim.apply_to(line));
                }
                let total_lines = body.lines().count();
                if total_lines > 20 {
                    println!(
                        "   {} (+{} more lines)",
                        dim.apply_to("..."),
                        total_lines - 20
                    );
                }
            } else if !skill.description.is_empty() {
                println!("   {}", dim.apply_to(&skill.description));
            }
        } else if !skill.description.is_empty() {
            println!("   {}", dim.apply_to(&skill.description));
        }

        println!();
    }

    // Let user pick one to install
    let names: Vec<String> = matches
        .iter()
        .map(|s| format!("{} ({})", s.name, s.marketplace))
        .collect();

    let selected = inquire::Select::new("Install a skill?", {
        let mut opts = names.clone();
        opts.push("Cancel".to_string());
        opts
    })
    .prompt()
    .context("selection cancelled")?;

    if selected == "Cancel" {
        return Ok(());
    }

    let idx = names.iter().position(|n| n == &selected).unwrap();
    let skill = matches[idx];

    // Offer to read full SKILL.md before installing
    if let Ok(content) = fs::read_to_string(&skill.skill_md_path) {
        let body = extract_skill_body(&content);
        if body.lines().count() > 20 {
            let show_full = Confirm::new("Show full SKILL.md before installing?")
                .with_default(false)
                .prompt()?;
            if show_full {
                println!("\n{}\n", content);
            }
        }
    }

    crate::skills::install::install_skill(config, Some(&skill.path))?;

    Ok(())
}

/// Extract the body of a SKILL.md (everything after the YAML frontmatter).
fn extract_skill_body(content: &str) -> &str {
    let mut in_frontmatter = false;
    let mut body_start = 0;

    for (i, line) in content.lines().enumerate() {
        if line.trim() == "---" {
            if in_frontmatter {
                // End of frontmatter — body starts after this line
                let offset: usize = content
                    .lines()
                    .take(i + 1)
                    .map(|l| l.len() + 1) // +1 for newline
                    .sum();
                body_start = offset.min(content.len());
                break;
            }
            in_frontmatter = true;
        }
    }

    content[body_start..].trim()
}
