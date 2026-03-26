use std::fs;

use anyhow::{bail, Context, Result};
use inquire::{Confirm, Select};

use crate::config::Config;
use crate::lockfile::SkillLock;
use crate::util::central_skills_dir;

/// Remove an installed skill by name (or prompt interactively).
pub fn remove_skill(config: &Config, name: Option<&str>) -> Result<()> {
    let central = central_skills_dir()?;

    let skill_name = match name {
        Some(n) => n.to_string(),
        None => prompt_skill_name(&central)?,
    };

    // Check skill exists in central
    let skill_dir = central.join(&skill_name);
    if !skill_dir.exists() {
        bail!("skill '{}' is not installed in {}", skill_name, central.display());
    }

    // Confirm removal
    let confirmed = Confirm::new(&format!("Remove skill '{}'?", skill_name))
        .with_default(false)
        .prompt()
        .context("confirmation cancelled")?;

    if !confirmed {
        println!("Cancelled.");
        return Ok(());
    }

    // Remove symlinks from all agent dirs
    let enabled = config.enabled_agents();
    for (id, agent) in &enabled {
        let link_path = agent.skills_path_expanded().join(&skill_name);
        if link_path.symlink_metadata().is_ok() {
            let meta = fs::symlink_metadata(&link_path)?;
            if meta.file_type().is_symlink() {
                fs::remove_file(&link_path).with_context(|| {
                    format!("failed to remove symlink {}", link_path.display())
                })?;
            } else {
                eprintln!(
                    "warning: {} in agent '{}' is not a symlink, skipping",
                    link_path.display(),
                    id
                );
            }
        }
    }

    // Remove from central dir
    fs::remove_dir_all(&skill_dir)
        .with_context(|| format!("failed to remove {}", skill_dir.display()))?;

    // Remove from lockfile
    let mut lock = SkillLock::load()?;
    lock.remove_skill(&skill_name);
    lock.save()?;

    println!("Removed skill '{}'.", skill_name);
    Ok(())
}

/// List skills in the central directory and prompt the user to select one.
fn prompt_skill_name(central: &std::path::Path) -> Result<String> {
    if !central.is_dir() {
        bail!("no skills installed (central skills directory does not exist)");
    }

    let mut skills: Vec<String> = Vec::new();
    for entry in fs::read_dir(central)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if let Some(name) = entry.file_name().to_str() {
                skills.push(name.to_string());
            }
        }
    }

    if skills.is_empty() {
        bail!("no skills installed");
    }

    skills.sort();

    let selected = Select::new("Select skill to remove:", skills)
        .prompt()
        .context("selection cancelled")?;

    Ok(selected)
}
