use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Metadata parsed from a SKILL.md YAML frontmatter.
#[derive(Debug, Clone)]
pub struct SkillMeta {
    pub name: String,
    pub description: String,
}

/// Expand a leading `~` in a path string to the user's home directory.
pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    } else if path == "~"
        && let Some(home) = dirs::home_dir() {
            return home;
        }
    PathBuf::from(path)
}

/// Returns `~/.agents/`.
pub fn agents_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("could not determine home directory")?;
    Ok(home.join(".agents"))
}

/// Returns `~/.agents/skills/`.
pub fn central_skills_dir() -> Result<PathBuf> {
    Ok(agents_dir()?.join("skills"))
}

/// Returns `~/.agents/cache/`.
pub fn cache_dir() -> Result<PathBuf> {
    Ok(agents_dir()?.join("cache"))
}

/// Parse SKILL.md YAML frontmatter for `name` and `description`.
///
/// Expects a file starting with `---`, followed by YAML key-value pairs,
/// closed by another `---`.
pub fn parse_skill_md(path: &Path) -> Result<SkillMeta> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;

    let mut in_frontmatter = false;
    let mut name: Option<String> = None;
    let mut description: Option<String> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "---" {
            if in_frontmatter {
                break; // end of frontmatter
            }
            in_frontmatter = true;
            continue;
        }
        if !in_frontmatter {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once(':') {
            let key = key.trim();
            let value = value.trim().trim_matches('"').trim_matches('\'');
            match key {
                "name" => name = Some(value.to_string()),
                "description" => description = Some(value.to_string()),
                _ => {}
            }
        }
    }

    Ok(SkillMeta {
        name: name.unwrap_or_else(|| {
            path.parent()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string())
        }),
        description: description.unwrap_or_default(),
    })
}

/// Create a relative symlink from `link_path` pointing to `target_path`.
///
/// Uses `pathdiff` to compute the relative path from the link's parent
/// directory to the target.
pub fn create_relative_symlink(target_path: &Path, link_path: &Path) -> Result<()> {
    let link_parent = link_path
        .parent()
        .context("link path has no parent directory")?;

    // Ensure the parent directory exists
    fs::create_dir_all(link_parent)
        .with_context(|| format!("failed to create directory {}", link_parent.display()))?;

    let rel_target = pathdiff::diff_paths(target_path, link_parent)
        .with_context(|| {
            format!(
                "could not compute relative path from {} to {}",
                link_parent.display(),
                target_path.display()
            )
        })?;

    // Remove existing symlink if present
    if link_path.symlink_metadata().is_ok() {
        fs::remove_file(link_path)
            .with_context(|| format!("failed to remove existing symlink {}", link_path.display()))?;
    }

    #[cfg(unix)]
    std::os::unix::fs::symlink(&rel_target, link_path)
        .with_context(|| {
            format!(
                "failed to create symlink {} -> {}",
                link_path.display(),
                rel_target.display()
            )
        })?;

    #[cfg(not(unix))]
    anyhow::bail!("symlinks are only supported on Unix systems");

    Ok(())
}

/// Check if `path` is a symlink that points into `~/.agents/skills/`.
pub fn is_central_symlink(path: &Path) -> bool {
    let Ok(target) = fs::read_link(path) else {
        return false;
    };

    // Resolve relative symlink target against the link's parent dir
    let resolved = if target.is_relative() {
        match path.parent() {
            Some(parent) => parent.join(&target),
            None => return false,
        }
    } else {
        target
    };

    // Canonicalize to resolve any .. components
    let Ok(canonical) = resolved.canonicalize() else {
        return false;
    };

    let Ok(skills_dir) = central_skills_dir() else {
        return false;
    };

    canonical.starts_with(skills_dir)
}
