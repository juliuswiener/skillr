use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Current lockfile schema version.
const LOCKFILE_VERSION: u32 = 3;

/// Represents the full `.skill-lock.json` lockfile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillLock {
    pub version: u32,
    pub skills: BTreeMap<String, SkillLockEntry>,
}

/// A single installed-skill entry in the lockfile.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillLockEntry {
    pub source: String,
    pub source_type: String,
    pub source_url: String,
    pub skill_path: String,
    pub skill_folder_hash: String,
    pub installed_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl SkillLock {
    /// Returns the path to the lockfile: `~/.agents/.skill-lock.json`.
    pub fn path() -> Result<PathBuf> {
        let home = dirs::home_dir().context("could not determine home directory")?;
        Ok(home.join(".agents").join(".skill-lock.json"))
    }

    /// Load lockfile from disk, or return an empty lockfile if the file doesn't exist.
    pub fn load() -> Result<SkillLock> {
        let path = Self::path()?;
        if !path.exists() {
            return Ok(SkillLock {
                version: LOCKFILE_VERSION,
                skills: BTreeMap::new(),
            });
        }
        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let lock: SkillLock = serde_json::from_str(&content)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        Ok(lock)
    }

    /// Serialize to pretty JSON and write atomically via temp file + rename.
    pub fn save(&self) -> Result<()> {
        let path = Self::path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }

        let content =
            serde_json::to_string_pretty(self).context("failed to serialize lockfile")?;

        let tmp_path = path.with_extension("json.tmp");
        {
            let mut f = fs::File::create(&tmp_path)
                .with_context(|| format!("failed to create temp file {}", tmp_path.display()))?;
            f.write_all(content.as_bytes())
                .with_context(|| format!("failed to write temp file {}", tmp_path.display()))?;
            f.write_all(b"\n")?;
            f.sync_all()?;
        }

        fs::rename(&tmp_path, &path).with_context(|| {
            format!(
                "failed to rename {} to {}",
                tmp_path.display(),
                path.display()
            )
        })?;

        Ok(())
    }

    /// Insert or update a skill entry with the current timestamp.
    pub fn add_skill(
        &mut self,
        name: &str,
        source: &str,
        source_url: &str,
        skill_path: &str,
    ) {
        let now = Utc::now();
        let entry = SkillLockEntry {
            source: source.to_string(),
            source_type: if source_url.is_empty() {
                "local".to_string()
            } else {
                "git".to_string()
            },
            source_url: source_url.to_string(),
            skill_path: skill_path.to_string(),
            skill_folder_hash: String::new(),
            installed_at: now,
            updated_at: now,
        };
        self.skills.insert(name.to_string(), entry);
    }

    /// Remove a skill entry by name.
    pub fn remove_skill(&mut self, name: &str) {
        self.skills.remove(name);
    }
}
