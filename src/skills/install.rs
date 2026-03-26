use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use inquire::{MultiSelect, Select, Text};
use walkdir::WalkDir;

use crate::config::Config;
use crate::lockfile::SkillLock;
use crate::util::{cache_dir, central_skills_dir, create_relative_symlink, parse_skill_md};

/// Install a skill from a source (GitHub repo, local path, or interactive menu).
pub fn install_skill(config: &Config, source: Option<&str>) -> Result<()> {
    let central = central_skills_dir()?;
    fs::create_dir_all(&central)
        .with_context(|| format!("failed to create {}", central.display()))?;

    let source_str = match source {
        Some(s) => s.to_string(),
        None => prompt_source()?,
    };

    let path = Path::new(&source_str);
    if path.is_dir() {
        install_from_local(config, path, &central)?;
    } else if source_str.contains('/') {
        install_from_github(config, &source_str, &central)?;
    } else {
        bail!(
            "cannot determine source type for '{}': not a local directory and doesn't look like a GitHub repo",
            source_str
        );
    }

    Ok(())
}

/// Show an interactive menu to choose the source type and get the source string.
fn prompt_source() -> Result<String> {
    let options = vec![
        "From GitHub (owner/repo)",
        "From local path",
    ];
    let choice = Select::new("Install skill from:", options)
        .prompt()
        .context("selection cancelled")?;

    match choice {
        "From GitHub (owner/repo)" => {
            let repo = Text::new("GitHub repo (owner/repo):")
                .prompt()
                .context("input cancelled")?;
            Ok(repo)
        }
        "From local path" => {
            let path = Text::new("Local path:")
                .prompt()
                .context("input cancelled")?;
            Ok(path)
        }
        _ => bail!("unexpected selection"),
    }
}

/// Install a skill from a local directory.
fn install_from_local(config: &Config, path: &Path, central: &Path) -> Result<()> {
    let skill_md = path.join("SKILL.md");
    if !skill_md.exists() {
        bail!("no SKILL.md found in {}", path.display());
    }

    let meta = parse_skill_md(&skill_md)?;
    let dest = central.join(&meta.name);

    println!("Installing skill '{}' from local path...", meta.name);
    copy_dir_recursive(path, &dest)?;

    // Update lockfile
    let mut lock = SkillLock::load()?;
    lock.add_skill(
        &meta.name,
        &path.to_string_lossy(),
        "",
        &dest.to_string_lossy(),
    );
    lock.save()?;

    symlink_to_agents(config, &meta.name, &dest)?;

    println!("Installed skill '{}'.", meta.name);
    Ok(())
}

/// Install skill(s) from a GitHub repository.
fn install_from_github(config: &Config, repo: &str, central: &Path) -> Result<()> {
    let cache = cache_dir()?;
    fs::create_dir_all(&cache)
        .with_context(|| format!("failed to create {}", cache.display()))?;

    let repo_dir_name = repo.replace('/', "-");
    let repo_path = cache.join(&repo_dir_name);
    let repo_url = format!("https://github.com/{}.git", repo);

    if repo_path.is_dir() {
        println!("Updating cached repository...");
        let status = Command::new("git")
            .args(["pull", "--quiet"])
            .current_dir(&repo_path)
            .status()
            .context("failed to run git pull")?;
        if !status.success() {
            bail!("git pull failed for {}", repo);
        }
    } else {
        println!("Cloning {}...", repo);
        let status = Command::new("git")
            .args(["clone", "--quiet", "--depth", "1", &repo_url, &repo_path.to_string_lossy()])
            .status()
            .context("failed to run git clone")?;
        if !status.success() {
            bail!("git clone failed for {}", repo);
        }
    }

    // Scan for skills in the repo
    let skills = scan_for_skills(&repo_path)?;
    if skills.is_empty() {
        bail!("no skills found in {}", repo);
    }

    // Let user select which skills to install
    let display_items: Vec<String> = skills
        .iter()
        .map(|(name, desc)| {
            if desc.is_empty() {
                name.clone()
            } else {
                format!("{} — {}", name, truncate(desc, 60))
            }
        })
        .collect();

    let defaults: Vec<usize> = (0..display_items.len()).collect();
    let selected = MultiSelect::new("Select skills to install:", display_items.clone())
        .with_default(&defaults)
        .prompt()
        .context("selection cancelled")?;

    let mut lock = SkillLock::load()?;

    for item in &selected {
        let idx = display_items.iter().position(|d| d == item).unwrap();
        let (skill_name, _) = &skills[idx];

        let skill_src = find_skill_dir(&repo_path, skill_name)?;
        let dest = central.join(skill_name);

        println!("Installing skill '{}'...", skill_name);
        copy_dir_recursive(&skill_src, &dest)?;

        lock.add_skill(
            skill_name,
            repo,
            &repo_url,
            &dest.to_string_lossy(),
        );

        symlink_to_agents(config, skill_name, &dest)?;
    }

    lock.save()?;

    let count = selected.len();
    println!("Installed {} skill{}.", count, if count == 1 { "" } else { "s" });
    Ok(())
}

/// Create symlinks from agent skill directories to the central skill directory.
fn symlink_to_agents(config: &Config, skill_name: &str, central_path: &Path) -> Result<()> {
    let enabled = config.enabled_agents();
    if enabled.is_empty() {
        return Ok(());
    }

    let agent_names: Vec<String> = enabled
        .iter()
        .map(|(id, agent)| format!("{} ({})", agent.name, id))
        .collect();

    let defaults: Vec<usize> = (0..agent_names.len()).collect();
    let selected = MultiSelect::new(
        &format!("Link '{}' to which agents?", skill_name),
        agent_names.clone(),
    )
    .with_default(&defaults)
    .prompt()
    .context("selection cancelled")?;

    let enabled_vec: Vec<(&String, &crate::agents::AgentConfig)> = enabled.into_iter().collect();

    for item in &selected {
        let idx = agent_names.iter().position(|n| n == item).unwrap();
        let (_id, agent) = &enabled_vec[idx];

        let agent_skills = agent.skills_path_expanded();
        fs::create_dir_all(&agent_skills)
            .with_context(|| format!("failed to create {}", agent_skills.display()))?;

        let link_path = agent_skills.join(skill_name);
        create_relative_symlink(central_path, &link_path)?;
    }

    Ok(())
}

/// Scan a repository directory for subdirectories containing SKILL.md.
/// Checks `skills/` subdir first, then root-level subdirs.
fn scan_for_skills(repo_dir: &Path) -> Result<Vec<(String, String)>> {
    let mut results = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // Check skills/ subdirectory first
    let skills_subdir = repo_dir.join("skills");
    if skills_subdir.is_dir() {
        for entry in fs::read_dir(&skills_subdir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let skill_md = entry.path().join("SKILL.md");
                if skill_md.exists() {
                    let meta = parse_skill_md(&skill_md)?;
                    if seen.insert(meta.name.clone()) {
                        results.push((meta.name, meta.description));
                    }
                }
            }
        }
    }

    // Check root-level: the repo itself might be a single skill
    let root_skill_md = repo_dir.join("SKILL.md");
    if root_skill_md.exists() {
        let meta = parse_skill_md(&root_skill_md)?;
        if seen.insert(meta.name.clone()) {
            results.push((meta.name, meta.description));
        }
    }

    // Check root-level subdirs (not skills/)
    if results.is_empty() || seen.len() < 2 {
        for entry in fs::read_dir(repo_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str == "skills" || name_str.starts_with('.') {
                    continue;
                }
                let skill_md = entry.path().join("SKILL.md");
                if skill_md.exists() {
                    let meta = parse_skill_md(&skill_md)?;
                    if seen.insert(meta.name.clone()) {
                        results.push((meta.name, meta.description));
                    }
                }
            }
        }
    }

    Ok(results)
}

/// Find the directory for a specific skill within a repo.
/// Checks skills/<name>, then <name>, then does a recursive walkdir search.
fn find_skill_dir(repo_dir: &Path, skill_name: &str) -> Result<PathBuf> {
    // Check skills/<name>/
    let candidate = repo_dir.join("skills").join(skill_name);
    if candidate.join("SKILL.md").exists() {
        return Ok(candidate);
    }

    // Check <name>/ at root
    let candidate = repo_dir.join(skill_name);
    if candidate.join("SKILL.md").exists() {
        return Ok(candidate);
    }

    // Check root itself (single-skill repo)
    let root_md = repo_dir.join("SKILL.md");
    if root_md.exists() {
        let meta = parse_skill_md(&root_md)?;
        if meta.name == skill_name {
            return Ok(repo_dir.to_path_buf());
        }
    }

    // Recursive search up to depth 4
    for entry in WalkDir::new(repo_dir).max_depth(4).into_iter().filter_map(|e| e.ok()) {
        if entry.file_name() == "SKILL.md" && entry.file_type().is_file() {
            if let Ok(meta) = parse_skill_md(entry.path()) {
                if meta.name == skill_name {
                    if let Some(parent) = entry.path().parent() {
                        return Ok(parent.to_path_buf());
                    }
                }
            }
        }
    }

    bail!("could not find skill directory for '{}'", skill_name);
}

/// Recursively copy a directory, skipping .git.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    if dst.exists() {
        fs::remove_dir_all(dst)
            .with_context(|| format!("failed to remove existing {}", dst.display()))?;
    }
    fs::create_dir_all(dst)
        .with_context(|| format!("failed to create {}", dst.display()))?;

    for entry in WalkDir::new(src).into_iter().filter_map(|e| e.ok()) {
        let rel = entry.path().strip_prefix(src).unwrap();

        // Skip .git directory
        if rel.components().any(|c| c.as_os_str() == ".git") {
            continue;
        }

        let target = dst.join(rel);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target)
                .with_context(|| format!("failed to create dir {}", target.display()))?;
        } else {
            fs::copy(entry.path(), &target).with_context(|| {
                format!(
                    "failed to copy {} to {}",
                    entry.path().display(),
                    target.display()
                )
            })?;
        }
    }

    Ok(())
}

/// Truncate a string to a maximum length, appending "..." if truncated.
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max.saturating_sub(3)])
    }
}
