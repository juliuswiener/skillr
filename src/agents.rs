use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::util::expand_tilde;

/// Format of an agent's MCP configuration file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum McpFormat {
    Json,
    Toml,
}

/// Configuration for a single AI agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Display name of the agent.
    pub name: String,

    /// Path to the agent's skills directory (may contain `~`).
    pub skills_path: String,

    /// Path to the agent's MCP config file (may contain `~`).
    pub mcp_config: Option<String>,

    /// Format of the MCP config file.
    pub mcp_format: McpFormat,

    /// Key within the MCP config file that holds the server map.
    pub mcp_key: Option<String>,

    /// Whether this agent is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

impl AgentConfig {
    /// Returns `skills_path` with `~` expanded to the home directory.
    pub fn skills_path_expanded(&self) -> PathBuf {
        expand_tilde(&self.skills_path)
    }

    /// Returns `mcp_config` with `~` expanded to the home directory.
    pub fn mcp_config_expanded(&self) -> Option<PathBuf> {
        self.mcp_config.as_ref().map(|p| expand_tilde(p))
    }
}

impl fmt::Display for AgentConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status = if self.enabled { "enabled" } else { "disabled" };
        write!(f, "{} ({})", self.name, status)
    }
}
