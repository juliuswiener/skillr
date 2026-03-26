use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::agents::{AgentConfig, McpFormat};

/// A marketplace source for discovering skills.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Marketplace {
    pub name: String,
    pub url: String,
}

/// Top-level configuration for skillr.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Registered agents, keyed by identifier.
    pub agents: BTreeMap<String, AgentConfig>,

    /// Marketplace sources.
    #[serde(default)]
    pub marketplaces: Vec<Marketplace>,
}

impl Config {
    /// Returns the path to the config file: `~/.agents/config.toml`.
    pub fn path() -> Result<PathBuf> {
        let home = dirs::home_dir().context("could not determine home directory")?;
        Ok(home.join(".agents").join("config.toml"))
    }

    /// Load config from disk, or return the default if the file doesn't exist.
    pub fn load() -> Result<Config> {
        let path = Self::path()?;
        if !path.exists() {
            return Ok(default_config());
        }
        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let config: Config = toml::from_str(&content)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        Ok(config)
    }

    /// Serialize to TOML and write atomically via temp file + rename.
    pub fn save(&self) -> Result<()> {
        let path = Self::path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }

        let content = toml::to_string_pretty(self).context("failed to serialize config")?;

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

    /// Returns only the agents that are enabled.
    pub fn enabled_agents(&self) -> BTreeMap<&String, &AgentConfig> {
        self.agents
            .iter()
            .filter(|(_, agent)| agent.enabled)
            .collect()
    }
}

/// Default configuration with three built-in agents.
pub fn default_config() -> Config {
    let mut agents = BTreeMap::new();

    agents.insert(
        "claude".to_string(),
        AgentConfig {
            name: "Claude".to_string(),
            skills_path: "~/.claude/skills".to_string(),
            mcp_config: Some("~/.claude/settings.json".to_string()),
            mcp_format: McpFormat::Json,
            mcp_key: Some("mcpServers".to_string()),
            enabled: true,
        },
    );

    agents.insert(
        "codex".to_string(),
        AgentConfig {
            name: "Codex".to_string(),
            skills_path: "~/.codex/skills".to_string(),
            mcp_config: Some("~/.codex/config.toml".to_string()),
            mcp_format: McpFormat::Toml,
            mcp_key: Some("mcp_servers".to_string()),
            enabled: true,
        },
    );

    agents.insert(
        "gemini".to_string(),
        AgentConfig {
            name: "Gemini".to_string(),
            skills_path: "~/.gemini/skills".to_string(),
            mcp_config: Some("~/.gemini/settings.json".to_string()),
            mcp_format: McpFormat::Json,
            mcp_key: Some("mcpServers".to_string()),
            enabled: true,
        },
    );

    Config {
        agents,
        marketplaces: Vec::new(),
    }
}
