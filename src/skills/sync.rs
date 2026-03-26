use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use console::Style;
use inquire::Confirm;

use crate::config::Config;
use crate::util::{central_skills_dir, create_relative_symlink, is_central_symlink};

/// Recursively copy a directory, skipping `.git` directories.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)
        .with_context(|| format!("failed to create directory {}", dst.display()))?;

    for entry in fs::read_dir(src)
        .with_context(|| format!("failed to read directory {}", src.display()))?
    {
        let entry = entry?;
        let file_name = entry.file_name();

        // Skip .git directories.
        if file_name == ".git" {
            continue;
        }

        let src_path = entry.path();
        let dst_path = dst.join(&file_name);
        let ft = entry.file_type()?;

        if ft.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path).with_context(|| {
                format!(
                    "failed to copy {} to {}",
                    src_path.display(),
                    dst_path.display()
                )
            })?;
        }
    }

    Ok(())
}

/// Bidirectional skill sync between agent directories and central skills dir.
pub fn sync_skills(config: &Config) -> Result<()> {
    let central = central_skills_dir()?;
    fs::create_dir_all(&central)
        .with_context(|| format!("failed to create {}", central.display()))?;

    let enabled = config.enabled_agents();
    let bold = Style::new().bold();
    let green = Style::new().green();
    let yellow = Style::new().yellow();
    let red = Style::new().red();

    // ── Phase 1: Centralize orphans from agent dirs ──

    println!("{}", bold.apply_to("Phase 1: Centralizing orphan skills from agent dirs..."));

    for (id, agent) in &enabled {
        let agent_skills = agent.skills_path_expanded();
        if !agent_skills.is_dir() {
            continue;
        }

        let entries: Vec<_> = fs::read_dir(&agent_skills)?
            .filter_map(|e| e.ok())
            .collect();

        for entry in entries {
            let skill_path = entry.path();
            let skill_name = entry.file_name();
            let skill_name_str = skill_name.to_string_lossy().to_string();

            // Skip entries that are already symlinks to central.
            if is_central_symlink(&skill_path) {
                continue;
            }

            let meta = fs::symlink_metadata(&skill_path);

            // Check for broken symlinks.
            if let Ok(m) = &meta
                && m.file_type().is_symlink() && !skill_path.exists() {
                    // Broken symlink.
                    let prompt = format!(
                        "  {} Broken symlink '{}' in {}. Remove it?",
                        red.apply_to("!"),
                        skill_name_str,
                        id
                    );
                    let remove = Confirm::new(&prompt)
                        .with_default(true)
                        .prompt()
                        .unwrap_or(false);
                    if remove {
                        fs::remove_file(&skill_path).with_context(|| {
                            format!("failed to remove broken symlink {}", skill_path.display())
                        })?;
                        println!("    {} Removed broken symlink.", green.apply_to("✓"));
                    }
                    continue;
                }

            // Real directory not in central.
            if skill_path.is_dir() {
                let central_dest = central.join(&skill_name);

                if central_dest.exists() {
                    // Conflict: exists in both central and agent as real dir.
                    println!(
                        "  {} Skill '{}' in {} conflicts with central copy — skipping.",
                        yellow.apply_to("⚠"),
                        skill_name_str,
                        id
                    );
                    continue;
                }

                let prompt = format!(
                    "  Skill '{}' found in {} but not centralized. Move to central?",
                    skill_name_str, id
                );
                let centralize = Confirm::new(&prompt)
                    .with_default(true)
                    .prompt()
                    .unwrap_or(false);

                if centralize {
                    // Copy to central.
                    copy_dir_recursive(&skill_path, &central_dest)?;
                    // Remove original directory.
                    fs::remove_dir_all(&skill_path).with_context(|| {
                        format!("failed to remove original dir {}", skill_path.display())
                    })?;
                    // Replace with symlink.
                    create_relative_symlink(&central_dest, &skill_path)?;
                    println!(
                        "    {} Centralized '{}' and created symlink.",
                        green.apply_to("✓"),
                        skill_name_str
                    );
                }
            }
        }
    }

    // ── Phase 2: Ensure central skills are linked into agents ──

    println!();
    println!(
        "{}",
        bold.apply_to("Phase 2: Linking central skills into agent dirs...")
    );

    if central.is_dir() {
        let central_entries: Vec<_> = fs::read_dir(&central)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .collect();

        for entry in &central_entries {
            let skill_name = entry.file_name();
            let skill_name_str = skill_name.to_string_lossy().to_string();
            let central_path = entry.path();

            for (id, agent) in &enabled {
                let agent_skills = agent.skills_path_expanded();
                let link_path = agent_skills.join(&skill_name);

                // Already linked correctly.
                if is_central_symlink(&link_path) {
                    continue;
                }

                // Already exists as something else — skip.
                if link_path.symlink_metadata().is_ok() {
                    continue;
                }

                let prompt = format!(
                    "  Link '{}' into {}?",
                    skill_name_str, id
                );
                let link = Confirm::new(&prompt)
                    .with_default(false)
                    .prompt()
                    .unwrap_or(false);

                if link {
                    create_relative_symlink(&central_path, &link_path)?;
                    println!(
                        "    {} Linked '{}' into {}.",
                        green.apply_to("✓"),
                        skill_name_str,
                        id
                    );
                }
            }
        }
    }

    println!();
    println!("{}", green.apply_to("Sync complete."));

    Ok(())
}
