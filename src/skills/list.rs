use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use anyhow::Result;
use console::Style;

use crate::config::Config;
use crate::lockfile::SkillLock;
use crate::util::{central_skills_dir, is_central_symlink};

/// Status of a skill within a specific agent's skills directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillStatus {
    /// Symlinked to central skills dir.
    Symlinked,
    /// Present as a real directory (not symlinked to central).
    Directory,
    /// Not present in this agent.
    Missing,
    /// Symlink exists but target is missing or broken.
    BrokenSymlink,
}

/// A row in the skills listing table.
#[derive(Debug, Clone)]
pub struct SkillRow {
    pub name: String,
    pub source: String,
    pub agent_status: BTreeMap<String, SkillStatus>,
}

/// Gather information about all installed skills across central and agent dirs.
pub fn gather_skills(config: &Config) -> Result<Vec<SkillRow>> {
    let central = central_skills_dir()?;
    let lock = SkillLock::load()?;
    let enabled = config.enabled_agents();

    // Collect all unique skill names from central dir and all agent dirs.
    let mut all_skills = BTreeSet::new();

    if central.is_dir() {
        for entry in fs::read_dir(&central)? {
            let entry = entry?;
            if entry.file_type()?.is_dir()
                && let Some(name) = entry.file_name().to_str() {
                    all_skills.insert(name.to_string());
                }
        }
    }

    for agent in enabled.values() {
        let agent_skills = agent.skills_path_expanded();
        if agent_skills.is_dir() {
            for entry in fs::read_dir(&agent_skills)? {
                let entry = entry?;
                let name_os = entry.file_name();
                if let Some(name) = name_os.to_str() {
                    all_skills.insert(name.to_string());
                }
            }
        }
    }

    // Build rows.
    let mut rows = Vec::new();
    for skill_name in &all_skills {
        let source = lock
            .skills
            .get(skill_name)
            .map(|e| e.source.clone())
            .unwrap_or_else(|| "local".to_string());

        let mut agent_status = BTreeMap::new();
        for (id, agent) in &enabled {
            let skill_path = agent.skills_path_expanded().join(skill_name);
            let status = if skill_path.symlink_metadata().is_ok() {
                // Entry exists (possibly as symlink).
                if is_central_symlink(&skill_path) {
                    SkillStatus::Symlinked
                } else if skill_path.is_dir() {
                    SkillStatus::Directory
                } else {
                    // Symlink but target is gone or not pointing to central.
                    let meta = fs::symlink_metadata(&skill_path);
                    if meta.map(|m| m.file_type().is_symlink()).unwrap_or(false) {
                        // It's a symlink but not to central — check if target exists.
                        if skill_path.exists() {
                            SkillStatus::Directory
                        } else {
                            SkillStatus::BrokenSymlink
                        }
                    } else {
                        SkillStatus::Missing
                    }
                }
            } else {
                SkillStatus::Missing
            };
            agent_status.insert(id.to_string(), status);
        }

        rows.push(SkillRow {
            name: skill_name.clone(),
            source,
            agent_status,
        });
    }

    Ok(rows)
}

/// Print a formatted table of skills and their status per agent.
pub fn print_skills_table(config: &Config) -> Result<()> {
    let rows = gather_skills(config)?;
    let enabled = config.enabled_agents();

    if rows.is_empty() {
        println!("No skills installed.");
        return Ok(());
    }

    let green = Style::new().green();
    let yellow = Style::new().yellow();
    let red = Style::new().red();
    let dim = Style::new().dim();

    // Compute column widths.
    let name_width = rows.iter().map(|r| r.name.len()).max().unwrap_or(5).max(5);
    let source_width = rows
        .iter()
        .map(|r| r.source.len())
        .max()
        .unwrap_or(6)
        .max(6);

    let agent_ids: Vec<&&String> = enabled.keys().collect();
    let agent_widths: Vec<usize> = agent_ids.iter().map(|id| id.len().max(5)).collect();

    // Header.
    print!("{:width$}  {:sw$}", "Skill", "Source", width = name_width, sw = source_width);
    for (i, id) in agent_ids.iter().enumerate() {
        print!("  {:width$}", id, width = agent_widths[i]);
    }
    println!();

    // Separator.
    print!("{:-<width$}  {:-<sw$}", "", "", width = name_width, sw = source_width);
    for (i, _) in agent_ids.iter().enumerate() {
        print!("  {:-<width$}", "", width = agent_widths[i]);
    }
    println!();

    // Rows.
    for row in &rows {
        print!(
            "{:width$}  {:sw$}",
            row.name,
            row.source,
            width = name_width,
            sw = source_width,
        );
        for (i, id) in agent_ids.iter().enumerate() {
            let status = row
                .agent_status
                .get(*id as &str)
                .unwrap_or(&SkillStatus::Missing);
            let cell = match status {
                SkillStatus::Symlinked => format!("{}", green.apply_to("✓ sym")),
                SkillStatus::Directory => format!("{}", yellow.apply_to("✓ dir")),
                SkillStatus::BrokenSymlink => format!("{}", red.apply_to("✗ broken")),
                SkillStatus::Missing => format!("{}", dim.apply_to("✗")),
            };
            print!("  {:>width$}", cell, width = agent_widths[i]);
        }
        println!();
    }

    Ok(())
}
