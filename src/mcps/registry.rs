use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::util::agents_dir;

/// A single MCP server entry in the central registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpEntry {
    /// The command to launch the MCP server.
    pub command: String,

    /// Arguments passed to the command.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,

    /// Environment variables for the server process.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,

    /// Which agents this MCP server should be available to.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agents: Vec<String>,
}

/// Central MCP server registry stored at `~/.agents/mcps.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpRegistry {
    #[serde(flatten)]
    pub servers: BTreeMap<String, McpEntry>,
}

impl McpRegistry {
    /// Returns the path to the registry file: `~/.agents/mcps.toml`.
    pub fn path() -> Result<PathBuf> {
        Ok(agents_dir()?.join("mcps.toml"))
    }

    /// Load the registry from disk, returning an empty default if the file is missing.
    pub fn load() -> Result<McpRegistry> {
        let path = Self::path()?;
        if !path.exists() {
            return Ok(McpRegistry::default());
        }
        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let registry: McpRegistry = toml::from_str(&content)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        Ok(registry)
    }

    /// Serialize to pretty TOML and write atomically via temp file + rename.
    pub fn save(&self) -> Result<()> {
        let path = Self::path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }

        let content = toml::to_string_pretty(self).context("failed to serialize MCP registry")?;

        let tmp_path = path.with_extension("toml.tmp");
        {
            let mut f = fs::File::create(&tmp_path)
                .with_context(|| format!("failed to create temp file {}", tmp_path.display()))?;
            f.write_all(content.as_bytes())
                .with_context(|| format!("failed to write temp file {}", tmp_path.display()))?;
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

    /// Return the servers that should be active for a given agent.
    ///
    /// A server matches if its `agents` list is empty (meaning all agents)
    /// or contains the given `agent_id`.
    pub fn servers_for_agent(&self, agent_id: &str) -> BTreeMap<String, &McpEntry> {
        self.servers
            .iter()
            .filter(|(_name, entry)| {
                entry.agents.is_empty() || entry.agents.iter().any(|a| a == agent_id)
            })
            .map(|(name, entry)| (name.clone(), entry))
            .collect()
    }
}
