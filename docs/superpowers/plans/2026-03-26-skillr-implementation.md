# skillr Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Rust CLI with interactive wizard menus that manages skills and MCP servers across Claude Code, Codex, and Gemini CLI agents.

**Architecture:** Central source of truth at `~/.agents/` with symlinks into each agent's native directory. Config in TOML, skill lock in JSON (playbooks-compatible). Interactive menus via `inquire`, CLI subcommands via `clap`.

**Tech Stack:** Rust, inquire, clap, serde/serde_json, toml_edit, git2, walkdir, console, anyhow

---

## File Structure

```
src/
├── main.rs              # Entry point: clap CLI parsing, dispatch to wizard or subcommands
├── config.rs            # Load/save ~/.agents/config.toml, agent definitions, marketplace list
├── agents.rs            # Agent trait + built-in agent definitions (claude, codex, gemini)
├── skills/
│   ├── mod.rs           # Re-exports
│   ├── install.rs       # Install skill from marketplace/github/local → central + symlink
│   ├── list.rs          # List skills with per-agent status table
│   ├── remove.rs        # Remove skill from central + clean symlinks
│   └── sync.rs          # Bidirectional sync: detect drift, centralize orphans, fix symlinks
├── mcps/
│   ├── mod.rs           # Re-exports
│   ├── registry.rs      # Load/save ~/.agents/mcps.toml central registry
│   ├── patch.rs         # Read/write agent MCP configs (JSON for Claude/Gemini, TOML for Codex)
│   ├── add.rs           # Add MCP to central + patch agent configs
│   ├── list.rs          # List MCPs with per-agent status
│   ├── remove.rs        # Remove MCP from central + patch agent configs
│   └── sync.rs          # Reconcile central registry ↔ agent configs
├── market/
│   ├── mod.rs           # Re-exports
│   ├── browse.rs        # Fuzzy search across cached marketplace skills
│   ├── manage.rs        # Add/remove/update/list marketplaces
│   └── cache.rs         # Clone/pull marketplace repos to ~/.agents/cache/
├── lockfile.rs          # Read/write .skill-lock.json (playbooks-compatible format)
├── wizard.rs            # Top-level interactive menu (Skills/MCPs/Marketplaces/Agents/Sync)
└── util.rs              # Shared helpers: path expansion, relative symlinks, SKILL.md parsing
```

---

### Task 1: Project Scaffold & Config Types

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `src/config.rs`
- Create: `src/agents.rs`
- Create: `src/util.rs`

- [ ] **Step 1: Initialize the Cargo project**

Run: `cd /home/julius/00_projects/skillr && cargo init`

- [ ] **Step 2: Set up Cargo.toml with dependencies**

Replace `Cargo.toml` with:

```toml
[package]
name = "skillr"
version = "0.1.0"
edition = "2024"
description = "Unified AI agent skill & MCP manager"

[dependencies]
inquire = "0.7"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
toml_edit = "0.22"
clap = { version = "4", features = ["derive"] }
walkdir = "2"
console = "0.15"
indicatif = "0.17"
dirs = "6"
git2 = "0.20"
sha2 = "0.10"
chrono = { version = "0.4", features = ["serde"] }
anyhow = "1"
```

- [ ] **Step 3: Write util.rs — path expansion and SKILL.md parsing**

Create `src/util.rs`:

```rust
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Expand ~ to home directory
pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

/// Get the ~/.agents/ base directory
pub fn agents_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Cannot determine home directory")?;
    Ok(home.join(".agents"))
}

/// Get the ~/.agents/skills/ directory
pub fn central_skills_dir() -> Result<PathBuf> {
    Ok(agents_dir()?.join("skills"))
}

/// Get the ~/.agents/cache/ directory
pub fn cache_dir() -> Result<PathBuf> {
    Ok(agents_dir()?.join("cache"))
}

/// Parse SKILL.md frontmatter to extract name and description
pub struct SkillMeta {
    pub name: String,
    pub description: String,
}

pub fn parse_skill_md(path: &Path) -> Result<SkillMeta> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;

    let mut name = String::new();
    let mut description = String::new();
    let mut in_frontmatter = false;
    let mut in_description = false;

    for line in content.lines() {
        if line.trim() == "---" {
            if in_frontmatter {
                break;
            }
            in_frontmatter = true;
            continue;
        }
        if !in_frontmatter {
            continue;
        }

        if let Some(val) = line.strip_prefix("name:") {
            name = val.trim().trim_matches('"').trim_matches('\'').to_string();
            in_description = false;
        } else if line.starts_with("description:") {
            let val = line.strip_prefix("description:").unwrap_or("");
            let val = val.trim();
            if val == ">" || val == "|" {
                in_description = true;
            } else {
                description = val.trim_matches('"').trim_matches('\'').to_string();
            }
        } else if in_description {
            let trimmed = line.trim();
            if trimmed.is_empty() || (!trimmed.starts_with(' ') && trimmed.contains(':')) {
                in_description = false;
            } else {
                if !description.is_empty() {
                    description.push(' ');
                }
                description.push_str(trimmed);
            }
        }
    }

    if name.is_empty() {
        // Fall back to directory name
        if let Some(parent) = path.parent() {
            name = parent
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
        }
    }

    Ok(SkillMeta { name, description })
}

/// Create a relative symlink from `link_path` pointing to `target_path`
pub fn create_relative_symlink(target_path: &Path, link_path: &Path) -> Result<()> {
    let link_parent = link_path
        .parent()
        .context("Symlink path has no parent directory")?;

    // Ensure the parent directory exists
    fs::create_dir_all(link_parent)?;

    let rel = pathdiff::diff_paths(target_path, link_parent)
        .unwrap_or_else(|| target_path.to_path_buf());

    #[cfg(unix)]
    std::os::unix::fs::symlink(&rel, link_path)
        .with_context(|| format!("Failed to symlink {} -> {}", link_path.display(), rel.display()))?;

    Ok(())
}

/// Check if a path is a symlink pointing into ~/.agents/skills/
pub fn is_central_symlink(path: &Path) -> bool {
    if !path.is_symlink() {
        return false;
    }
    match fs::read_link(path) {
        Ok(target) => {
            let resolved = if target.is_relative() {
                path.parent()
                    .map(|p| p.join(&target))
                    .unwrap_or(target.clone())
            } else {
                target.clone()
            };
            // Canonicalize both to compare
            let central = agents_dir().ok().map(|d| d.join("skills"));
            if let (Ok(resolved), Some(central)) = (resolved.canonicalize(), central) {
                if let Ok(central) = central.canonicalize() {
                    return resolved.starts_with(&central);
                }
            }
            false
        }
        Err(_) => false,
    }
}
```

- [ ] **Step 4: Add pathdiff dependency**

Add to `Cargo.toml` under `[dependencies]`:

```toml
pathdiff = "0.2"
```

- [ ] **Step 5: Write agents.rs — agent definition types**

Create `src/agents.rs`:

```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::util::expand_tilde;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub name: String,
    pub skills_path: String,
    #[serde(default)]
    pub mcp_config: Option<String>,
    #[serde(default = "default_mcp_format")]
    pub mcp_format: McpFormat,
    #[serde(default)]
    pub mcp_key: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

fn default_mcp_format() -> McpFormat {
    McpFormat::Json
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum McpFormat {
    Json,
    Toml,
}

impl AgentConfig {
    pub fn skills_path_expanded(&self) -> PathBuf {
        expand_tilde(&self.skills_path)
    }

    pub fn mcp_config_expanded(&self) -> Option<PathBuf> {
        self.mcp_config.as_ref().map(|p| expand_tilde(p))
    }
}

impl std::fmt::Display for AgentConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}
```

- [ ] **Step 6: Write config.rs — load/save config.toml**

Create `src/config.rs`:

```rust
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use crate::agents::AgentConfig;
use crate::util::agents_dir;

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub agents: BTreeMap<String, AgentConfig>,
    #[serde(default)]
    pub marketplaces: Vec<Marketplace>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Marketplace {
    pub name: String,
    pub url: String,
}

impl Config {
    pub fn path() -> Result<PathBuf> {
        Ok(agents_dir()?.join("config.toml"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::path()?;
        if !path.exists() {
            return Ok(Self::default_config());
        }
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        let config: Config = toml::from_str(&content)
            .with_context(|| format!("Failed to parse {}", path.display()))?;
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path()?;
        let dir = path.parent().unwrap();
        fs::create_dir_all(dir)?;

        let content = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;

        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, &content)?;
        fs::rename(&tmp, &path)?;
        Ok(())
    }

    pub fn enabled_agents(&self) -> Vec<(&String, &AgentConfig)> {
        self.agents
            .iter()
            .filter(|(_, a)| a.enabled)
            .collect()
    }

    fn default_config() -> Self {
        let mut agents = BTreeMap::new();
        agents.insert(
            "claude".to_string(),
            AgentConfig {
                name: "Claude Code".to_string(),
                skills_path: "~/.claude/skills".to_string(),
                mcp_config: Some("~/.claude/settings.json".to_string()),
                mcp_format: crate::agents::McpFormat::Json,
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
                mcp_format: crate::agents::McpFormat::Toml,
                mcp_key: Some("mcp_servers".to_string()),
                enabled: true,
            },
        );
        agents.insert(
            "gemini".to_string(),
            AgentConfig {
                name: "Gemini CLI".to_string(),
                skills_path: "~/.gemini/skills".to_string(),
                mcp_config: Some("~/.gemini/settings.json".to_string()),
                mcp_format: crate::agents::McpFormat::Json,
                mcp_key: Some("mcpServers".to_string()),
                enabled: true,
            },
        );
        Self {
            agents,
            marketplaces: vec![],
        }
    }
}
```

- [ ] **Step 7: Write minimal main.rs with clap skeleton**

Create `src/main.rs`:

```rust
mod agents;
mod config;
mod util;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "skillr", about = "Unified AI agent skill & MCP manager")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Sync all skills and MCPs
    Sync,
    /// Manage skills
    Skills {
        #[command(subcommand)]
        action: SkillsAction,
    },
    /// Manage MCP servers
    Mcps {
        #[command(subcommand)]
        action: McpsAction,
    },
    /// Manage marketplaces
    Market {
        #[command(subcommand)]
        action: MarketAction,
    },
}

#[derive(Subcommand)]
enum SkillsAction {
    /// List installed skills
    List,
    /// Install a skill
    Install {
        /// GitHub repo (owner/repo) or local path
        source: Option<String>,
    },
    /// Remove a skill
    Remove {
        /// Skill name
        name: Option<String>,
    },
    /// Sync skills across agents
    Sync,
}

#[derive(Subcommand)]
enum McpsAction {
    /// List MCP servers
    List,
    /// Add an MCP server
    Add {
        /// MCP server name
        name: Option<String>,
    },
    /// Remove an MCP server
    Remove {
        /// MCP server name
        name: Option<String>,
    },
    /// Sync MCP configs across agents
    Sync,
    /// Import MCPs from an agent's config
    Import,
}

#[derive(Subcommand)]
enum MarketAction {
    /// Browse marketplace skills
    Browse {
        /// Search query
        query: Option<String>,
    },
    /// Add a marketplace
    Add {
        /// GitHub repo URL (owner/repo)
        repo: String,
    },
    /// Update marketplace caches
    Update,
    /// List registered marketplaces
    List,
    /// Remove a marketplace
    Remove {
        /// Marketplace name
        name: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        None => {
            // Launch interactive wizard
            println!("skillr - Unified AI Agent Skill & MCP Manager");
            println!("(wizard not yet implemented)");
            Ok(())
        }
        Some(Commands::Sync) => {
            println!("(sync not yet implemented)");
            Ok(())
        }
        Some(Commands::Skills { action }) => {
            match action {
                SkillsAction::List => println!("(skills list not yet implemented)"),
                SkillsAction::Install { .. } => println!("(skills install not yet implemented)"),
                SkillsAction::Remove { .. } => println!("(skills remove not yet implemented)"),
                SkillsAction::Sync => println!("(skills sync not yet implemented)"),
            }
            Ok(())
        }
        Some(Commands::Mcps { action }) => {
            match action {
                McpsAction::List => println!("(mcps list not yet implemented)"),
                McpsAction::Add { .. } => println!("(mcps add not yet implemented)"),
                McpsAction::Remove { .. } => println!("(mcps remove not yet implemented)"),
                McpsAction::Sync => println!("(mcps sync not yet implemented)"),
                McpsAction::Import => println!("(mcps import not yet implemented)"),
            }
            Ok(())
        }
        Some(Commands::Market { action }) => {
            match action {
                MarketAction::Browse { .. } => println!("(market browse not yet implemented)"),
                MarketAction::Add { .. } => println!("(market add not yet implemented)"),
                MarketAction::Update => println!("(market update not yet implemented)"),
                MarketAction::List => println!("(market list not yet implemented)"),
                MarketAction::Remove { .. } => println!("(market remove not yet implemented)"),
            }
            Ok(())
        }
    }
}
```

- [ ] **Step 8: Verify it compiles**

Run: `cd /home/julius/00_projects/skillr && cargo build`
Expected: Compiles with no errors.

- [ ] **Step 9: Commit scaffold**

```bash
cd /home/julius/00_projects/skillr
git add Cargo.toml Cargo.lock src/
git commit -m "feat: project scaffold with CLI skeleton, config, agents, util"
```

---

### Task 2: Lockfile (`.skill-lock.json`)

**Files:**
- Create: `src/lockfile.rs`
- Modify: `src/main.rs` (add `mod lockfile`)

- [ ] **Step 1: Write lockfile.rs**

Create `src/lockfile.rs`:

```rust
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use crate::util::agents_dir;

const LOCK_VERSION: u32 = 3;

#[derive(Debug, Serialize, Deserialize)]
pub struct SkillLock {
    pub version: u32,
    #[serde(default)]
    pub skills: BTreeMap<String, SkillLockEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillLockEntry {
    pub source: String,
    pub source_type: String,
    pub source_url: String,
    pub skill_path: String,
    #[serde(default)]
    pub skill_folder_hash: String,
    pub installed_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl SkillLock {
    pub fn path() -> Result<PathBuf> {
        Ok(agents_dir()?.join(".skill-lock.json"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::path()?;
        if !path.exists() {
            return Ok(Self {
                version: LOCK_VERSION,
                skills: BTreeMap::new(),
            });
        }
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        let lock: SkillLock = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse {}", path.display()))?;
        Ok(lock)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path()?;
        let dir = path.parent().unwrap();
        fs::create_dir_all(dir)?;

        let content = serde_json::to_string_pretty(self)
            .context("Failed to serialize skill lock")?;

        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, &content)?;
        fs::rename(&tmp, &path)?;
        Ok(())
    }

    pub fn add_skill(
        &mut self,
        name: &str,
        source: &str,
        source_url: &str,
        skill_path: &str,
    ) {
        let now = Utc::now();
        self.skills.insert(
            name.to_string(),
            SkillLockEntry {
                source: source.to_string(),
                source_type: "github".to_string(),
                source_url: source_url.to_string(),
                skill_path: skill_path.to_string(),
                skill_folder_hash: String::new(),
                installed_at: now,
                updated_at: now,
            },
        );
    }

    pub fn remove_skill(&mut self, name: &str) {
        self.skills.remove(name);
    }
}
```

- [ ] **Step 2: Add mod lockfile to main.rs**

Add `mod lockfile;` to the top of `src/main.rs` after the existing mod declarations.

- [ ] **Step 3: Verify it compiles**

Run: `cd /home/julius/00_projects/skillr && cargo build`
Expected: Compiles with no errors.

- [ ] **Step 4: Commit**

```bash
cd /home/julius/00_projects/skillr
git add src/lockfile.rs src/main.rs
git commit -m "feat: add skill lockfile read/write (playbooks-compatible)"
```

---

### Task 3: Skills — List & Sync

**Files:**
- Create: `src/skills/mod.rs`
- Create: `src/skills/list.rs`
- Create: `src/skills/sync.rs`
- Modify: `src/main.rs` (add `mod skills`, wire up commands)

- [ ] **Step 1: Create src/skills/mod.rs**

```rust
pub mod list;
pub mod sync;
pub mod install;
pub mod remove;
```

- [ ] **Step 2: Write skills/list.rs — list installed skills with agent status**

Create `src/skills/list.rs`:

```rust
use anyhow::Result;
use console::Style;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use crate::config::Config;
use crate::lockfile::SkillLock;
use crate::util::{central_skills_dir, is_central_symlink};

#[derive(Debug)]
pub enum SkillStatus {
    Symlinked,
    Directory,  // exists but not a symlink to central
    Missing,
    BrokenSymlink,
}

pub struct SkillRow {
    pub name: String,
    pub source: String,
    pub agent_status: BTreeMap<String, SkillStatus>,
}

pub fn gather_skills(config: &Config) -> Result<Vec<SkillRow>> {
    let central = central_skills_dir()?;
    let lock = SkillLock::load()?;

    // Collect all known skill names
    let mut all_skills: BTreeSet<String> = BTreeSet::new();

    // From central directory
    if central.exists() {
        for entry in fs::read_dir(&central)? {
            let entry = entry?;
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    all_skills.insert(name.to_string());
                }
            }
        }
    }

    // From each agent's skills dir
    let agents = config.enabled_agents();
    for (_, agent) in &agents {
        let agent_skills = agent.skills_path_expanded();
        if !agent_skills.exists() {
            continue;
        }
        for entry in fs::read_dir(&agent_skills)? {
            let entry = entry?;
            if let Some(name) = entry.file_name().to_str() {
                all_skills.insert(name.to_string());
            }
        }
    }

    // Build rows
    let mut rows = Vec::new();
    for skill_name in &all_skills {
        let source = lock
            .skills
            .get(skill_name)
            .map(|e| e.source.clone())
            .unwrap_or_else(|| {
                if central.join(skill_name).exists() {
                    "local".to_string()
                } else {
                    "—".to_string()
                }
            });

        let mut agent_status = BTreeMap::new();
        for (agent_id, agent) in &agents {
            let skill_path = agent.skills_path_expanded().join(skill_name);
            let status = if !skill_path.exists() && !skill_path.is_symlink() {
                SkillStatus::Missing
            } else if skill_path.is_symlink() {
                if is_central_symlink(&skill_path) {
                    SkillStatus::Symlinked
                } else if !skill_path.exists() {
                    SkillStatus::BrokenSymlink
                } else {
                    SkillStatus::Symlinked
                }
            } else if skill_path.is_dir() {
                SkillStatus::Directory
            } else {
                SkillStatus::Missing
            };
            agent_status.insert(agent_id.to_string(), status);
        }

        rows.push(SkillRow {
            name: skill_name.clone(),
            source,
            agent_status,
        });
    }

    Ok(rows)
}

pub fn print_skills_table(config: &Config) -> Result<()> {
    let rows = gather_skills(config)?;
    let agents = config.enabled_agents();

    if rows.is_empty() {
        println!("No skills installed.");
        return Ok(());
    }

    let green = Style::new().green();
    let red = Style::new().red();
    let yellow = Style::new().yellow();
    let dim = Style::new().dim();

    // Header
    print!("{:<24} {:<24}", "Skill", "Source");
    for (id, _) in &agents {
        print!(" {:<12}", id);
    }
    println!();
    print!("{:<24} {:<24}", "─".repeat(22), "─".repeat(22));
    for _ in &agents {
        print!(" {:<12}", "─".repeat(10));
    }
    println!();

    // Rows
    for row in &rows {
        print!("{:<24} {:<24}", row.name, dim.apply_to(&row.source));
        for (id, _) in &agents {
            let status = row.agent_status.get(id.as_str());
            let cell = match status {
                Some(SkillStatus::Symlinked) => green.apply_to("✓ sym".to_string()),
                Some(SkillStatus::Directory) => yellow.apply_to("✓ dir".to_string()),
                Some(SkillStatus::BrokenSymlink) => red.apply_to("✗ broken".to_string()),
                Some(SkillStatus::Missing) | None => dim.apply_to("✗".to_string()),
            };
            print!(" {:<12}", cell);
        }
        println!();
    }

    Ok(())
}
```

- [ ] **Step 3: Write skills/sync.rs — bidirectional sync**

Create `src/skills/sync.rs`:

```rust
use anyhow::{Context, Result};
use console::Style;
use inquire::Confirm;
use std::fs;

use crate::config::Config;
use crate::util::{central_skills_dir, create_relative_symlink, is_central_symlink};

pub fn sync_skills(config: &Config) -> Result<()> {
    let central = central_skills_dir()?;
    fs::create_dir_all(&central)?;

    let green = Style::new().green();
    let yellow = Style::new().yellow();
    let red = Style::new().red();

    let agents = config.enabled_agents();
    let mut changes = 0;

    // Phase 1: Centralize orphan skills from agent dirs
    for (agent_id, agent) in &agents {
        let agent_skills = agent.skills_path_expanded();
        if !agent_skills.exists() {
            continue;
        }

        for entry in fs::read_dir(&agent_skills)? {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            // Skip if already a symlink to central
            if is_central_symlink(&path) {
                continue;
            }

            // Skip broken symlinks
            if path.is_symlink() && !path.exists() {
                println!(
                    "  {} Broken symlink: {}/{}",
                    red.apply_to("✗"),
                    agent_id,
                    name_str
                );
                let remove = Confirm::new(&format!("Remove broken symlink {}/{}?", agent_id, name_str))
                    .with_default(true)
                    .prompt()?;
                if remove {
                    fs::remove_file(&path)?;
                    changes += 1;
                }
                continue;
            }

            // Real directory not in central — offer to centralize
            if path.is_dir() {
                let central_dest = central.join(&name);
                if central_dest.exists() {
                    println!(
                        "  {} {}/{} exists in both agent and central (conflict — skipping)",
                        yellow.apply_to("⚠"),
                        agent_id,
                        name_str
                    );
                    continue;
                }

                let centralize = Confirm::new(&format!(
                    "Centralize {}/{} → ~/.agents/skills/{} and symlink back?",
                    agent_id, name_str, name_str
                ))
                .with_default(true)
                .prompt()?;

                if centralize {
                    // Move to central
                    fs_extra_copy_dir(&path, &central_dest)?;
                    fs::remove_dir_all(&path)?;
                    // Create symlink back
                    create_relative_symlink(&central_dest, &path)?;
                    println!(
                        "  {} Centralized {}/{}",
                        green.apply_to("✓"),
                        agent_id,
                        name_str
                    );
                    changes += 1;
                }
            }
        }
    }

    // Phase 2: Ensure central skills are symlinked into all agent dirs
    if central.exists() {
        for entry in fs::read_dir(&central)? {
            let entry = entry?;
            if !entry.path().is_dir() {
                continue;
            }
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            for (agent_id, agent) in &agents {
                let agent_skills = agent.skills_path_expanded();
                let link_path = agent_skills.join(&name);

                if link_path.exists() || link_path.is_symlink() {
                    continue; // Already exists (symlink or dir)
                }

                let create = Confirm::new(&format!(
                    "Create symlink for '{}' in {}?",
                    name_str, agent_id
                ))
                .with_default(false)
                .prompt()?;

                if create {
                    fs::create_dir_all(&agent_skills)?;
                    create_relative_symlink(&entry.path(), &link_path)?;
                    println!(
                        "  {} Linked {} → {}",
                        green.apply_to("✓"),
                        name_str,
                        agent_id
                    );
                    changes += 1;
                }
            }
        }
    }

    if changes == 0 {
        println!("Everything in sync.");
    } else {
        println!("\n{} change(s) applied.", changes);
    }

    Ok(())
}

/// Recursively copy a directory (simple implementation)
fn fs_extra_copy_dir(src: &std::path::Path, dst: &std::path::Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            fs_extra_copy_dir(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)
                .with_context(|| format!("Failed to copy {}", src_path.display()))?;
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Create stub files for install.rs and remove.rs**

Create `src/skills/install.rs`:

```rust
use anyhow::Result;

use crate::config::Config;

pub fn install_skill(_config: &Config, _source: Option<&str>) -> Result<()> {
    println!("(install not yet implemented)");
    Ok(())
}
```

Create `src/skills/remove.rs`:

```rust
use anyhow::Result;

use crate::config::Config;

pub fn remove_skill(_config: &Config, _name: Option<&str>) -> Result<()> {
    println!("(remove not yet implemented)");
    Ok(())
}
```

- [ ] **Step 5: Wire up in main.rs**

Add `mod skills;` to `src/main.rs` and update the `Skills` match arm:

```rust
Some(Commands::Skills { action }) => {
    let config = config::Config::load()?;
    match action {
        SkillsAction::List => skills::list::print_skills_table(&config)?,
        SkillsAction::Install { source } => {
            skills::install::install_skill(&config, source.as_deref())?
        }
        SkillsAction::Remove { name } => {
            skills::remove::remove_skill(&config, name.as_deref())?
        }
        SkillsAction::Sync => skills::sync::sync_skills(&config)?,
    }
    Ok(())
}
```

Also update `Commands::Sync`:

```rust
Some(Commands::Sync) => {
    let config = config::Config::load()?;
    skills::sync::sync_skills(&config)?;
    Ok(())
}
```

- [ ] **Step 6: Verify it compiles**

Run: `cd /home/julius/00_projects/skillr && cargo build`
Expected: Compiles with no errors.

- [ ] **Step 7: Commit**

```bash
cd /home/julius/00_projects/skillr
git add src/skills/
git commit -m "feat: add skills list and bidirectional sync"
```

---

### Task 4: Skills — Install & Remove

**Files:**
- Modify: `src/skills/install.rs`
- Modify: `src/skills/remove.rs`

- [ ] **Step 1: Implement install.rs — install from local path or GitHub**

Replace `src/skills/install.rs`:

```rust
use anyhow::{bail, Context, Result};
use console::Style;
use inquire::{MultiSelect, Select, Text};
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::config::Config;
use crate::lockfile::SkillLock;
use crate::util::{central_skills_dir, create_relative_symlink, parse_skill_md, cache_dir};

pub fn install_skill(config: &Config, source: Option<&str>) -> Result<()> {
    let source = match source {
        Some(s) => s.to_string(),
        None => {
            let options = vec![
                "From GitHub (owner/repo)",
                "From local path",
            ];
            let choice = Select::new("Install from:", options).prompt()?;
            match choice {
                "From GitHub (owner/repo)" => {
                    Text::new("GitHub repo (owner/repo):").prompt()?
                }
                "From local path" => {
                    Text::new("Path to skill directory:").prompt()?
                }
                _ => unreachable!(),
            }
        }
    };

    let green = Style::new().green();
    let central = central_skills_dir()?;
    fs::create_dir_all(&central)?;

    let path = Path::new(&source);
    if path.exists() && path.is_dir() {
        // Local install
        install_from_local(config, path, &central)?;
    } else if source.contains('/') {
        // GitHub install
        install_from_github(config, &source, &central)?;
    } else {
        bail!("Cannot determine source type for '{}'", source);
    }

    println!("  {} Done!", green.apply_to("✓"));
    Ok(())
}

fn install_from_local(config: &Config, path: &Path, central: &Path) -> Result<()> {
    let skill_md = path.join("SKILL.md");
    if !skill_md.exists() {
        bail!("No SKILL.md found in {}", path.display());
    }

    let meta = parse_skill_md(&skill_md)?;
    let dest = central.join(&meta.name);

    if dest.exists() {
        bail!("Skill '{}' already exists in central directory", meta.name);
    }

    // Copy to central
    copy_dir_recursive(path, &dest)?;
    println!("Installed '{}' to central skills", meta.name);

    // Symlink to agents
    symlink_to_agents(config, &meta.name, &dest)?;

    Ok(())
}

fn install_from_github(config: &Config, repo: &str, central: &Path) -> Result<()> {
    let cache = cache_dir()?;
    fs::create_dir_all(&cache)?;

    let repo_name = repo.replace('/', "-");
    let repo_dir = cache.join(&repo_name);

    // Clone or pull
    if repo_dir.exists() {
        println!("Updating {}...", repo);
        Command::new("git")
            .args(["pull", "--quiet"])
            .current_dir(&repo_dir)
            .status()
            .context("Failed to git pull")?;
    } else {
        let url = if repo.starts_with("http") {
            repo.to_string()
        } else {
            format!("https://github.com/{}.git", repo)
        };
        println!("Cloning {}...", repo);
        Command::new("git")
            .args(["clone", "--quiet", "--depth", "1", &url, repo_dir.to_str().unwrap()])
            .status()
            .context("Failed to git clone")?;
    }

    // Scan for skills
    let skills = scan_for_skills(&repo_dir)?;
    if skills.is_empty() {
        bail!("No skills found in {}", repo);
    }

    // Let user select which skills to install
    let display: Vec<String> = skills
        .iter()
        .map(|(name, desc)| {
            if desc.is_empty() {
                name.clone()
            } else {
                format!("{} — {}", name, truncate(desc, 60))
            }
        })
        .collect();

    let selected = MultiSelect::new("Select skills to install:", display.clone())
        .prompt()?;

    let mut lock = SkillLock::load()?;
    let source_url = format!("https://github.com/{}.git", repo);

    for sel in &selected {
        let idx = display.iter().position(|d| d == sel).unwrap();
        let (skill_name, _) = &skills[idx];

        let skill_src = find_skill_dir(&repo_dir, skill_name)?;
        let dest = central.join(skill_name);

        if dest.exists() {
            println!("Skill '{}' already exists — skipping", skill_name);
            continue;
        }

        copy_dir_recursive(&skill_src, &dest)?;
        println!("Installed '{}'", skill_name);

        // Update lock
        let skill_path = skill_src
            .strip_prefix(&repo_dir)
            .unwrap_or(&skill_src)
            .join("SKILL.md");
        lock.add_skill(
            skill_name,
            repo,
            &source_url,
            &skill_path.to_string_lossy(),
        );

        // Symlink to agents
        symlink_to_agents(config, skill_name, &dest)?;
    }

    lock.save()?;
    Ok(())
}

fn symlink_to_agents(config: &Config, skill_name: &str, central_path: &Path) -> Result<()> {
    let agents = config.enabled_agents();
    let agent_names: Vec<String> = agents.iter().map(|(id, _)| id.to_string()).collect();

    let selected = MultiSelect::new("Install to which agents?", agent_names.clone())
        .with_default(&(0..agent_names.len()).collect::<Vec<_>>())
        .prompt()?;

    for agent_id in &selected {
        let (_, agent) = agents
            .iter()
            .find(|(id, _)| *id == agent_id)
            .unwrap();
        let agent_skills = agent.skills_path_expanded();
        fs::create_dir_all(&agent_skills)?;
        let link = agent_skills.join(skill_name);
        if link.exists() || link.is_symlink() {
            continue;
        }
        create_relative_symlink(central_path, &link)?;
        println!("  Linked → {}", agent_id);
    }

    Ok(())
}

/// Scan a repo for skill directories containing SKILL.md
fn scan_for_skills(repo_dir: &Path) -> Result<Vec<(String, String)>> {
    let mut skills = Vec::new();
    let search_dirs = ["skills", ".", ""];

    for subdir in &search_dirs {
        let dir = if subdir.is_empty() {
            repo_dir.to_path_buf()
        } else {
            repo_dir.join(subdir)
        };
        if !dir.exists() {
            continue;
        }
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            let skill_md = path.join("SKILL.md");
            if skill_md.exists() {
                let meta = parse_skill_md(&skill_md)?;
                if !skills.iter().any(|(n, _): &(String, String)| n == &meta.name) {
                    skills.push((meta.name, meta.description));
                }
            }
        }
        if !skills.is_empty() {
            break; // Found skills in this search dir, don't recurse further
        }
    }

    Ok(skills)
}

fn find_skill_dir(repo_dir: &Path, skill_name: &str) -> Result<std::path::PathBuf> {
    // Check skills/<name> first, then root-level <name>
    let candidates = [
        repo_dir.join("skills").join(skill_name),
        repo_dir.join(skill_name),
    ];
    for c in &candidates {
        if c.join("SKILL.md").exists() {
            return Ok(c.clone());
        }
    }
    // Recursive fallback
    for entry in walkdir::WalkDir::new(repo_dir).max_depth(4) {
        let entry = entry?;
        if entry.file_name() == "SKILL.md" {
            if let Some(parent) = entry.path().parent() {
                let dir_name = parent
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                if dir_name == skill_name {
                    return Ok(parent.to_path_buf());
                }
                // Also check frontmatter name
                if let Ok(meta) = parse_skill_md(entry.path()) {
                    if meta.name == skill_name {
                        return Ok(parent.to_path_buf());
                    }
                }
            }
        }
    }
    bail!("Could not find skill directory for '{}'", skill_name)
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            // Skip .git directories
            if entry.file_name() == ".git" {
                continue;
            }
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max - 3])
    }
}
```

- [ ] **Step 2: Implement remove.rs**

Replace `src/skills/remove.rs`:

```rust
use anyhow::{bail, Result};
use console::Style;
use inquire::{Confirm, Select};
use std::fs;

use crate::config::Config;
use crate::lockfile::SkillLock;
use crate::util::central_skills_dir;

pub fn remove_skill(config: &Config, name: Option<&str>) -> Result<()> {
    let central = central_skills_dir()?;

    let skill_name = match name {
        Some(n) => n.to_string(),
        None => {
            // List available skills for selection
            let mut skills: Vec<String> = Vec::new();
            if central.exists() {
                for entry in fs::read_dir(&central)? {
                    let entry = entry?;
                    if entry.path().is_dir() {
                        if let Some(name) = entry.file_name().to_str() {
                            skills.push(name.to_string());
                        }
                    }
                }
            }
            if skills.is_empty() {
                bail!("No skills installed");
            }
            skills.sort();
            Select::new("Select skill to remove:", skills).prompt()?
        }
    };

    let skill_dir = central.join(&skill_name);
    if !skill_dir.exists() {
        bail!("Skill '{}' not found in central directory", skill_name);
    }

    let confirm = Confirm::new(&format!("Remove skill '{}'?", skill_name))
        .with_default(false)
        .prompt()?;

    if !confirm {
        println!("Cancelled.");
        return Ok(());
    }

    let green = Style::new().green();

    // Remove symlinks from all agents
    for (agent_id, agent) in config.enabled_agents() {
        let link = agent.skills_path_expanded().join(&skill_name);
        if link.is_symlink() {
            fs::remove_file(&link)?;
            println!("  {} Removed symlink from {}", green.apply_to("✓"), agent_id);
        } else if link.exists() {
            println!("  ⚠ {}/{} is not a symlink — skipping", agent_id, skill_name);
        }
    }

    // Remove from central
    fs::remove_dir_all(&skill_dir)?;
    println!("  {} Removed from central", green.apply_to("✓"));

    // Remove from lockfile
    let mut lock = SkillLock::load()?;
    lock.remove_skill(&skill_name);
    lock.save()?;

    Ok(())
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cd /home/julius/00_projects/skillr && cargo build`
Expected: Compiles with no errors.

- [ ] **Step 4: Commit**

```bash
cd /home/julius/00_projects/skillr
git add src/skills/
git commit -m "feat: add skills install (github + local) and remove"
```

---

### Task 5: MCP Registry & Config Patching

**Files:**
- Create: `src/mcps/mod.rs`
- Create: `src/mcps/registry.rs`
- Create: `src/mcps/patch.rs`
- Create: `src/mcps/add.rs`
- Create: `src/mcps/list.rs`
- Create: `src/mcps/remove.rs`
- Create: `src/mcps/sync.rs`
- Modify: `src/main.rs` (add `mod mcps`, wire up)

- [ ] **Step 1: Create src/mcps/mod.rs**

```rust
pub mod registry;
pub mod patch;
pub mod add;
pub mod list;
pub mod remove;
pub mod sync;
```

- [ ] **Step 2: Write registry.rs — central mcps.toml read/write**

Create `src/mcps/registry.rs`:

```rust
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use crate::util::agents_dir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpEntry {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub agents: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct McpRegistry {
    #[serde(flatten)]
    pub servers: BTreeMap<String, McpEntry>,
}

impl McpRegistry {
    pub fn path() -> Result<PathBuf> {
        Ok(agents_dir()?.join("mcps.toml"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        let registry: McpRegistry = toml::from_str(&content)
            .with_context(|| format!("Failed to parse {}", path.display()))?;
        Ok(registry)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path()?;
        let dir = path.parent().unwrap();
        fs::create_dir_all(dir)?;

        let content = toml::to_string_pretty(self)
            .context("Failed to serialize MCP registry")?;

        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, &content)?;
        fs::rename(&tmp, &path)?;
        Ok(())
    }

    pub fn servers_for_agent(&self, agent_id: &str) -> BTreeMap<String, &McpEntry> {
        self.servers
            .iter()
            .filter(|(_, entry)| entry.agents.contains(&agent_id.to_string()))
            .map(|(name, entry)| (name.clone(), entry))
            .collect()
    }
}
```

- [ ] **Step 3: Write patch.rs — read/write agent MCP configs**

Create `src/mcps/patch.rs`:

```rust
use anyhow::{bail, Context, Result};
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::agents::{AgentConfig, McpFormat};
use crate::mcps::registry::McpEntry;

/// Read all MCPs from an agent's config file
pub fn read_agent_mcps(agent: &AgentConfig) -> Result<BTreeMap<String, McpEntry>> {
    let config_path = match agent.mcp_config_expanded() {
        Some(p) => p,
        None => return Ok(BTreeMap::new()),
    };

    if !config_path.exists() {
        return Ok(BTreeMap::new());
    }

    match agent.mcp_format {
        McpFormat::Json => read_json_mcps(&config_path, agent.mcp_key.as_deref().unwrap_or("mcpServers")),
        McpFormat::Toml => read_toml_mcps(&config_path, agent.mcp_key.as_deref().unwrap_or("mcp_servers")),
    }
}

/// Write/update a single MCP in an agent's config
pub fn write_agent_mcp(agent: &AgentConfig, name: &str, entry: &McpEntry) -> Result<()> {
    let config_path = match agent.mcp_config_expanded() {
        Some(p) => p,
        None => bail!("Agent {} has no MCP config path", agent.name),
    };

    // Backup
    if config_path.exists() {
        let bak = config_path.with_extension("bak");
        fs::copy(&config_path, &bak)?;
    }

    match agent.mcp_format {
        McpFormat::Json => write_json_mcp(&config_path, agent.mcp_key.as_deref().unwrap_or("mcpServers"), name, entry),
        McpFormat::Toml => write_toml_mcp(&config_path, agent.mcp_key.as_deref().unwrap_or("mcp_servers"), name, entry),
    }
}

/// Remove a single MCP from an agent's config
pub fn remove_agent_mcp(agent: &AgentConfig, name: &str) -> Result<()> {
    let config_path = match agent.mcp_config_expanded() {
        Some(p) => p,
        None => bail!("Agent {} has no MCP config path", agent.name),
    };

    if !config_path.exists() {
        return Ok(());
    }

    // Backup
    let bak = config_path.with_extension("bak");
    fs::copy(&config_path, &bak)?;

    match agent.mcp_format {
        McpFormat::Json => remove_json_mcp(&config_path, agent.mcp_key.as_deref().unwrap_or("mcpServers"), name),
        McpFormat::Toml => remove_toml_mcp(&config_path, agent.mcp_key.as_deref().unwrap_or("mcp_servers"), name),
    }
}

// --- JSON helpers ---

fn read_json_mcps(path: &Path, key: &str) -> Result<BTreeMap<String, McpEntry>> {
    let content = fs::read_to_string(path)?;
    let root: JsonValue = serde_json::from_str(&content)?;

    let servers = match root.get(key) {
        Some(obj) => obj,
        None => return Ok(BTreeMap::new()),
    };

    let mut result = BTreeMap::new();
    if let Some(map) = servers.as_object() {
        for (name, val) in map {
            let command = val.get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let args: Vec<String> = val.get("args")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|a| a.as_str().map(String::from)).collect())
                .unwrap_or_default();
            let env: BTreeMap<String, String> = val.get("env")
                .and_then(|v| v.as_object())
                .map(|obj| {
                    obj.iter()
                        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                        .collect()
                })
                .unwrap_or_default();

            result.insert(name.clone(), McpEntry {
                command,
                args,
                env,
                agents: vec![],
            });
        }
    }

    Ok(result)
}

fn write_json_mcp(path: &Path, key: &str, name: &str, entry: &McpEntry) -> Result<()> {
    let content = if path.exists() {
        fs::read_to_string(path)?
    } else {
        "{}".to_string()
    };
    let mut root: serde_json::Map<String, JsonValue> = serde_json::from_str(&content)?;

    let servers = root
        .entry(key.to_string())
        .or_insert_with(|| JsonValue::Object(serde_json::Map::new()));

    let mcp_obj = servers
        .as_object_mut()
        .context("MCP key is not an object")?;

    let mut server = serde_json::Map::new();
    server.insert("command".to_string(), JsonValue::String(entry.command.clone()));
    server.insert(
        "args".to_string(),
        JsonValue::Array(entry.args.iter().map(|a| JsonValue::String(a.clone())).collect()),
    );
    if !entry.env.is_empty() {
        let env_obj: serde_json::Map<String, JsonValue> = entry
            .env
            .iter()
            .map(|(k, v)| (k.clone(), JsonValue::String(v.clone())))
            .collect();
        server.insert("env".to_string(), JsonValue::Object(env_obj));
    }

    mcp_obj.insert(name.to_string(), JsonValue::Object(server));

    let output = serde_json::to_string_pretty(&root)?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, &output)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

fn remove_json_mcp(path: &Path, key: &str, name: &str) -> Result<()> {
    let content = fs::read_to_string(path)?;
    let mut root: serde_json::Map<String, JsonValue> = serde_json::from_str(&content)?;

    if let Some(servers) = root.get_mut(key) {
        if let Some(obj) = servers.as_object_mut() {
            obj.remove(name);
        }
    }

    let output = serde_json::to_string_pretty(&root)?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, &output)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

// --- TOML helpers ---

fn read_toml_mcps(path: &Path, key: &str) -> Result<BTreeMap<String, McpEntry>> {
    let content = fs::read_to_string(path)?;
    let root: toml::Value = content.parse()?;

    let mut result = BTreeMap::new();
    if let Some(servers) = root.get(key).and_then(|v| v.as_table()) {
        for (name, val) in servers {
            // Skip sub-tables that are env sections (e.g., mcp_servers.foo.env)
            let command = val.get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if command.is_empty() {
                continue; // This is a sub-table, not a server entry
            }

            let args: Vec<String> = val.get("args")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|a| a.as_str().map(String::from)).collect())
                .unwrap_or_default();

            let env: BTreeMap<String, String> = val.get("env")
                .and_then(|v| v.as_table())
                .map(|tbl| {
                    tbl.iter()
                        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                        .collect()
                })
                .unwrap_or_default();

            result.insert(name.clone(), McpEntry {
                command,
                args,
                env,
                agents: vec![],
            });
        }
    }

    Ok(result)
}

fn write_toml_mcp(path: &Path, key: &str, name: &str, entry: &McpEntry) -> Result<()> {
    let content = if path.exists() {
        fs::read_to_string(path)?
    } else {
        String::new()
    };

    let mut doc: toml_edit::DocumentMut = content.parse()
        .context("Failed to parse TOML")?;

    // Ensure the key section exists
    if doc.get(key).is_none() {
        doc[key] = toml_edit::Item::Table(toml_edit::Table::new());
    }

    let full_key = format!("{}.{}", key, name);
    doc[key][name] = toml_edit::Item::Table(toml_edit::Table::new());
    doc[key][name]["command"] = toml_edit::value(&entry.command);

    let mut arr = toml_edit::Array::new();
    for arg in &entry.args {
        arr.push(arg.as_str());
    }
    doc[key][name]["args"] = toml_edit::value(arr);

    if !entry.env.is_empty() {
        doc[key][name]["env"] = toml_edit::Item::Table(toml_edit::Table::new());
        for (k, v) in &entry.env {
            doc[key][name]["env"][k.as_str()] = toml_edit::value(v.as_str());
        }
    }

    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, doc.to_string())?;
    fs::rename(&tmp, path)?;
    Ok(())
}

fn remove_toml_mcp(path: &Path, key: &str, name: &str) -> Result<()> {
    let content = fs::read_to_string(path)?;
    let mut doc: toml_edit::DocumentMut = content.parse()?;

    if let Some(servers) = doc.get_mut(key).and_then(|v| v.as_table_mut()) {
        servers.remove(name);
    }

    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, doc.to_string())?;
    fs::rename(&tmp, path)?;
    Ok(())
}
```

- [ ] **Step 4: Write add.rs — add MCP interactively**

Create `src/mcps/add.rs`:

```rust
use anyhow::Result;
use console::Style;
use inquire::{MultiSelect, Text};

use crate::config::Config;
use crate::mcps::patch::write_agent_mcp;
use crate::mcps::registry::{McpEntry, McpRegistry};

pub fn add_mcp(config: &Config, name: Option<&str>) -> Result<()> {
    let green = Style::new().green();

    let name = match name {
        Some(n) => n.to_string(),
        None => Text::new("MCP server name:").prompt()?,
    };

    let command = Text::new("Command (e.g., npx, uvx):").prompt()?;

    let args_str = Text::new("Arguments (space-separated):").prompt()?;
    let args: Vec<String> = args_str
        .split_whitespace()
        .map(String::from)
        .collect();

    let mut env = std::collections::BTreeMap::new();
    loop {
        let env_pair = Text::new("Environment variable (KEY=VALUE, empty to skip):")
            .prompt()?;
        if env_pair.is_empty() {
            break;
        }
        if let Some((k, v)) = env_pair.split_once('=') {
            env.insert(k.to_string(), v.to_string());
        } else {
            println!("Invalid format — use KEY=VALUE");
        }
    }

    let agent_names: Vec<String> = config
        .enabled_agents()
        .iter()
        .map(|(id, _)| id.to_string())
        .collect();

    let selected = MultiSelect::new("Enable for which agents?", agent_names.clone())
        .with_default(&(0..agent_names.len()).collect::<Vec<_>>())
        .prompt()?;

    let entry = McpEntry {
        command,
        args,
        env,
        agents: selected.clone(),
    };

    // Save to central registry
    let mut registry = McpRegistry::load()?;
    registry.servers.insert(name.clone(), entry.clone());
    registry.save()?;
    println!("  {} Saved to central registry", green.apply_to("✓"));

    // Patch agent configs
    for agent_id in &selected {
        let (_, agent) = config
            .enabled_agents()
            .into_iter()
            .find(|(id, _)| *id == agent_id)
            .unwrap();
        write_agent_mcp(agent, &name, &entry)?;
        println!("  {} Patched {}", green.apply_to("✓"), agent_id);
    }

    Ok(())
}
```

- [ ] **Step 5: Write list.rs — list MCPs with agent status**

Create `src/mcps/list.rs`:

```rust
use anyhow::Result;
use console::Style;
use std::collections::BTreeSet;

use crate::config::Config;
use crate::mcps::patch::read_agent_mcps;
use crate::mcps::registry::McpRegistry;

pub fn list_mcps(config: &Config) -> Result<()> {
    let registry = McpRegistry::load()?;
    let agents = config.enabled_agents();

    let green = Style::new().green();
    let dim = Style::new().dim();

    // Gather all MCP names (from central + all agents)
    let mut all_names: BTreeSet<String> = registry.servers.keys().cloned().collect();
    let mut agent_mcps = Vec::new();

    for (agent_id, agent) in &agents {
        let mcps = read_agent_mcps(agent)?;
        for name in mcps.keys() {
            all_names.insert(name.clone());
        }
        agent_mcps.push((agent_id.to_string(), mcps));
    }

    if all_names.is_empty() {
        println!("No MCP servers configured.");
        return Ok(());
    }

    // Header
    print!("{:<28} {:<20}", "MCP Server", "Command");
    for (id, _) in &agents {
        print!(" {:<10}", id);
    }
    println!();
    print!("{:<28} {:<20}", "─".repeat(26), "─".repeat(18));
    for _ in &agents {
        print!(" {:<10}", "─".repeat(8));
    }
    println!();

    for name in &all_names {
        let cmd = registry
            .servers
            .get(name)
            .map(|e| e.command.as_str())
            .unwrap_or("—");
        print!("{:<28} {:<20}", name, dim.apply_to(cmd));

        for (agent_id, mcps) in &agent_mcps {
            let mark = if mcps.contains_key(name) {
                green.apply_to("✓".to_string())
            } else {
                dim.apply_to("✗".to_string())
            };
            print!(" {:<10}", mark);
        }
        println!();
    }

    Ok(())
}
```

- [ ] **Step 6: Write remove.rs**

Create `src/mcps/remove.rs`:

```rust
use anyhow::{bail, Result};
use console::Style;
use inquire::{Confirm, Select};

use crate::config::Config;
use crate::mcps::patch::remove_agent_mcp;
use crate::mcps::registry::McpRegistry;

pub fn remove_mcp(config: &Config, name: Option<&str>) -> Result<()> {
    let mut registry = McpRegistry::load()?;
    let green = Style::new().green();

    let mcp_name = match name {
        Some(n) => n.to_string(),
        None => {
            let names: Vec<String> = registry.servers.keys().cloned().collect();
            if names.is_empty() {
                bail!("No MCP servers configured");
            }
            Select::new("Select MCP to remove:", names).prompt()?
        }
    };

    if !registry.servers.contains_key(&mcp_name) {
        bail!("MCP '{}' not found in central registry", mcp_name);
    }

    let confirm = Confirm::new(&format!("Remove MCP '{}'?", mcp_name))
        .with_default(false)
        .prompt()?;

    if !confirm {
        println!("Cancelled.");
        return Ok(());
    }

    // Remove from agent configs
    let entry = &registry.servers[&mcp_name];
    for agent_id in &entry.agents {
        if let Some((_, agent)) = config.enabled_agents().into_iter().find(|(id, _)| *id == agent_id) {
            remove_agent_mcp(agent, &mcp_name)?;
            println!("  {} Removed from {}", green.apply_to("✓"), agent_id);
        }
    }

    // Remove from central
    registry.servers.remove(&mcp_name);
    registry.save()?;
    println!("  {} Removed from central registry", green.apply_to("✓"));

    Ok(())
}
```

- [ ] **Step 7: Write sync.rs — reconcile central ↔ agents**

Create `src/mcps/sync.rs`:

```rust
use anyhow::Result;
use console::Style;
use inquire::Confirm;

use crate::config::Config;
use crate::mcps::patch::{read_agent_mcps, write_agent_mcp, remove_agent_mcp};
use crate::mcps::registry::{McpEntry, McpRegistry};

pub fn sync_mcps(config: &Config) -> Result<()> {
    let mut registry = McpRegistry::load()?;
    let agents = config.enabled_agents();

    let green = Style::new().green();
    let yellow = Style::new().yellow();
    let mut changes = 0;

    for (agent_id, agent) in &agents {
        println!("\nSyncing {}...", agent.name);
        let agent_mcps = read_agent_mcps(agent)?;

        // Check central MCPs that should be in this agent
        let central_for_agent = registry.servers_for_agent(agent_id);
        for (name, entry) in &central_for_agent {
            if !agent_mcps.contains_key(name) {
                let push = Confirm::new(&format!(
                    "  Push '{}' to {}?",
                    name, agent_id
                ))
                .with_default(true)
                .prompt()?;

                if push {
                    write_agent_mcp(agent, name, entry)?;
                    println!("  {} Pushed '{}'", green.apply_to("✓"), name);
                    changes += 1;
                }
            }
        }

        // Check agent MCPs not in central
        for (name, agent_entry) in &agent_mcps {
            if !registry.servers.contains_key(name) {
                let import = Confirm::new(&format!(
                    "  Import '{}' from {} to central?",
                    name, agent_id
                ))
                .with_default(true)
                .prompt()?;

                if import {
                    let entry = McpEntry {
                        command: agent_entry.command.clone(),
                        args: agent_entry.args.clone(),
                        env: agent_entry.env.clone(),
                        agents: vec![agent_id.to_string()],
                    };
                    registry.servers.insert(name.clone(), entry);
                    println!("  {} Imported '{}'", green.apply_to("✓"), name);
                    changes += 1;
                }
            }
        }
    }

    if changes > 0 {
        registry.save()?;
        println!("\n{} change(s) applied.", changes);
    } else {
        println!("\nAll MCPs in sync.");
    }

    Ok(())
}
```

- [ ] **Step 8: Wire up in main.rs**

Add `mod mcps;` to `src/main.rs` and update the `Mcps` match arm:

```rust
Some(Commands::Mcps { action }) => {
    let config = config::Config::load()?;
    match action {
        McpsAction::List => mcps::list::list_mcps(&config)?,
        McpsAction::Add { name } => mcps::add::add_mcp(&config, name.as_deref())?,
        McpsAction::Remove { name } => mcps::remove::remove_mcp(&config, name.as_deref())?,
        McpsAction::Sync => mcps::sync::sync_mcps(&config)?,
        McpsAction::Import => mcps::sync::sync_mcps(&config)?,
    }
    Ok(())
}
```

Also update `Commands::Sync` to include MCP sync:

```rust
Some(Commands::Sync) => {
    let config = config::Config::load()?;
    println!("=== Syncing Skills ===");
    skills::sync::sync_skills(&config)?;
    println!("\n=== Syncing MCPs ===");
    mcps::sync::sync_mcps(&config)?;
    Ok(())
}
```

- [ ] **Step 9: Verify it compiles**

Run: `cd /home/julius/00_projects/skillr && cargo build`
Expected: Compiles with no errors.

- [ ] **Step 10: Commit**

```bash
cd /home/julius/00_projects/skillr
git add src/mcps/ src/main.rs
git commit -m "feat: add MCP registry, config patching, add/list/remove/sync"
```

---

### Task 6: Marketplace Management

**Files:**
- Create: `src/market/mod.rs`
- Create: `src/market/cache.rs`
- Create: `src/market/browse.rs`
- Create: `src/market/manage.rs`
- Modify: `src/main.rs` (add `mod market`, wire up)

- [ ] **Step 1: Create src/market/mod.rs**

```rust
pub mod cache;
pub mod browse;
pub mod manage;
```

- [ ] **Step 2: Write cache.rs — clone/pull marketplace repos**

Create `src/market/cache.rs`:

```rust
use anyhow::{Context, Result};
use console::Style;
use std::fs;
use std::process::Command;

use crate::config::Marketplace;
use crate::util::cache_dir;

/// Get the local cache path for a marketplace
pub fn marketplace_cache_path(marketplace: &Marketplace) -> Result<std::path::PathBuf> {
    let cache = cache_dir()?;
    let dir_name = marketplace.name.replace('/', "-");
    Ok(cache.join(dir_name))
}

/// Clone or update a marketplace repo
pub fn update_marketplace(marketplace: &Marketplace) -> Result<()> {
    let green = Style::new().green();
    let cache = cache_dir()?;
    fs::create_dir_all(&cache)?;

    let repo_dir = marketplace_cache_path(marketplace)?;

    if repo_dir.exists() {
        println!("Updating {}...", marketplace.name);
        Command::new("git")
            .args(["pull", "--quiet"])
            .current_dir(&repo_dir)
            .status()
            .context("Failed to git pull")?;
    } else {
        let url = if marketplace.url.starts_with("http") {
            marketplace.url.clone()
        } else {
            format!("https://github.com/{}.git", marketplace.url)
        };
        println!("Cloning {}...", marketplace.name);
        Command::new("git")
            .args([
                "clone",
                "--quiet",
                "--depth",
                "1",
                &url,
                repo_dir.to_str().unwrap(),
            ])
            .status()
            .context("Failed to git clone")?;
    }

    println!("  {} {}", green.apply_to("✓"), marketplace.name);
    Ok(())
}

/// Update all registered marketplaces
pub fn update_all_marketplaces(marketplaces: &[Marketplace]) -> Result<()> {
    if marketplaces.is_empty() {
        println!("No marketplaces registered. Use 'skillr market add <repo>' to add one.");
        return Ok(());
    }

    for mp in marketplaces {
        update_marketplace(mp)?;
    }

    println!("\nAll marketplaces updated.");
    Ok(())
}
```

- [ ] **Step 3: Write browse.rs — fuzzy search marketplace skills**

Create `src/market/browse.rs`:

```rust
use anyhow::{bail, Result};
use inquire::Select;
use std::fs;
use walkdir::WalkDir;

use crate::config::Config;
use crate::market::cache::marketplace_cache_path;
use crate::skills::install::install_skill;
use crate::util::parse_skill_md;

struct MarketSkill {
    name: String,
    description: String,
    marketplace: String,
    path: std::path::PathBuf,
}

impl std::fmt::Display for MarketSkill {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.description.is_empty() {
            write!(f, "{} ({})", self.name, self.marketplace)
        } else {
            let desc = if self.description.len() > 50 {
                format!("{}...", &self.description[..47])
            } else {
                self.description.clone()
            };
            write!(f, "{} — {} ({})", self.name, desc, self.marketplace)
        }
    }
}

pub fn browse_marketplace(config: &Config, query: Option<&str>) -> Result<()> {
    if config.marketplaces.is_empty() {
        bail!("No marketplaces registered. Use 'skillr market add <repo>' first.");
    }

    let mut all_skills: Vec<MarketSkill> = Vec::new();

    for mp in &config.marketplaces {
        let cache_path = marketplace_cache_path(mp)?;
        if !cache_path.exists() {
            println!("Marketplace '{}' not cached — run 'skillr market update' first", mp.name);
            continue;
        }

        // Scan for SKILL.md files
        for entry in WalkDir::new(&cache_path).max_depth(5) {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            if entry.file_name() != "SKILL.md" {
                continue;
            }
            if let Ok(meta) = parse_skill_md(entry.path()) {
                let skill_dir = entry.path().parent().unwrap().to_path_buf();
                all_skills.push(MarketSkill {
                    name: meta.name,
                    description: meta.description,
                    marketplace: mp.name.clone(),
                    path: skill_dir,
                });
            }
        }
    }

    if all_skills.is_empty() {
        bail!("No skills found in cached marketplaces.");
    }

    // Filter by query if provided
    if let Some(q) = query {
        let q_lower = q.to_lowercase();
        all_skills.retain(|s| {
            s.name.to_lowercase().contains(&q_lower)
                || s.description.to_lowercase().contains(&q_lower)
        });
        if all_skills.is_empty() {
            bail!("No skills matching '{}'", q);
        }
    }

    let selected = Select::new("Select a skill to install:", all_skills).prompt()?;

    // Install using the local path
    install_skill(config, Some(selected.path.to_str().unwrap()))?;

    Ok(())
}
```

- [ ] **Step 4: Write manage.rs — add/remove/list marketplaces**

Create `src/market/manage.rs`:

```rust
use anyhow::{bail, Result};
use console::Style;
use inquire::{Select, Text};

use crate::config::{Config, Marketplace};
use crate::market::cache::update_marketplace;

pub fn add_marketplace(config: &mut Config, repo: &str) -> Result<()> {
    let green = Style::new().green();

    let name = if repo.contains('/') && !repo.contains("://") {
        repo.to_string()
    } else {
        // Extract owner/repo from URL
        repo.trim_end_matches(".git")
            .rsplit('/')
            .take(2)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("/")
    };

    let url = if repo.starts_with("http") {
        repo.to_string()
    } else {
        format!("https://github.com/{}", repo)
    };

    if config.marketplaces.iter().any(|m| m.name == name) {
        bail!("Marketplace '{}' already registered", name);
    }

    let mp = Marketplace {
        name: name.clone(),
        url,
    };

    // Clone it
    update_marketplace(&mp)?;

    config.marketplaces.push(mp);
    config.save()?;

    println!("  {} Added marketplace '{}'", green.apply_to("✓"), name);
    Ok(())
}

pub fn list_marketplaces(config: &Config) -> Result<()> {
    if config.marketplaces.is_empty() {
        println!("No marketplaces registered.");
        return Ok(());
    }

    let dim = Style::new().dim();
    println!("{:<30} {}", "Name", "URL");
    println!("{:<30} {}", "─".repeat(28), "─".repeat(40));
    for mp in &config.marketplaces {
        println!("{:<30} {}", mp.name, dim.apply_to(&mp.url));
    }
    Ok(())
}

pub fn remove_marketplace(config: &mut Config, name: Option<&str>) -> Result<()> {
    let green = Style::new().green();

    let mp_name = match name {
        Some(n) => n.to_string(),
        None => {
            let names: Vec<String> = config.marketplaces.iter().map(|m| m.name.clone()).collect();
            if names.is_empty() {
                bail!("No marketplaces registered");
            }
            Select::new("Select marketplace to remove:", names).prompt()?
        }
    };

    config.marketplaces.retain(|m| m.name != mp_name);
    config.save()?;
    println!("  {} Removed marketplace '{}'", green.apply_to("✓"), mp_name);
    Ok(())
}
```

- [ ] **Step 5: Wire up in main.rs**

Add `mod market;` and update the `Market` match arm:

```rust
Some(Commands::Market { action }) => {
    let mut config = config::Config::load()?;
    match action {
        MarketAction::Browse { query } => {
            market::browse::browse_marketplace(&config, query.as_deref())?
        }
        MarketAction::Add { repo } => {
            market::manage::add_marketplace(&mut config, &repo)?
        }
        MarketAction::Update => {
            market::cache::update_all_marketplaces(&config.marketplaces)?
        }
        MarketAction::List => market::manage::list_marketplaces(&config)?,
        MarketAction::Remove { name } => {
            market::manage::remove_marketplace(&mut config, name.as_deref())?
        }
    }
    Ok(())
}
```

- [ ] **Step 6: Verify it compiles**

Run: `cd /home/julius/00_projects/skillr && cargo build`
Expected: Compiles with no errors.

- [ ] **Step 7: Commit**

```bash
cd /home/julius/00_projects/skillr
git add src/market/ src/main.rs
git commit -m "feat: add marketplace browse, add, update, list, remove"
```

---

### Task 7: Interactive Wizard

**Files:**
- Create: `src/wizard.rs`
- Modify: `src/main.rs` (wire wizard into `None` command branch)

- [ ] **Step 1: Write wizard.rs — top-level interactive menu**

Create `src/wizard.rs`:

```rust
use anyhow::Result;
use inquire::Select;

use crate::config::Config;
use crate::market;
use crate::mcps;
use crate::skills;

pub fn run_wizard() -> Result<()> {
    println!("skillr — Unified AI Agent Skill & MCP Manager\n");

    loop {
        let mut config = Config::load()?;

        let options = vec![
            "Skills",
            "MCPs",
            "Marketplaces",
            "Agents",
            "Sync All",
            "Exit",
        ];

        let choice = Select::new("What would you like to do?", options).prompt()?;

        match choice {
            "Skills" => skills_menu(&config)?,
            "MCPs" => mcps_menu(&config)?,
            "Marketplaces" => marketplaces_menu(&mut config)?,
            "Agents" => agents_menu(&mut config)?,
            "Sync All" => {
                println!("\n=== Syncing Skills ===");
                skills::sync::sync_skills(&config)?;
                println!("\n=== Syncing MCPs ===");
                mcps::sync::sync_mcps(&config)?;
            }
            "Exit" => break,
            _ => unreachable!(),
        }

        println!();
    }

    Ok(())
}

fn skills_menu(config: &Config) -> Result<()> {
    let options = vec![
        "Install skill",
        "List installed",
        "Remove skill",
        "Sync (detect drift)",
        "← Back",
    ];

    let choice = Select::new("Skills >", options).prompt()?;

    match choice {
        "Install skill" => skills::install::install_skill(config, None)?,
        "List installed" => skills::list::print_skills_table(config)?,
        "Remove skill" => skills::remove::remove_skill(config, None)?,
        "Sync (detect drift)" => skills::sync::sync_skills(config)?,
        "← Back" => {}
        _ => unreachable!(),
    }

    Ok(())
}

fn mcps_menu(config: &Config) -> Result<()> {
    let options = vec![
        "Add MCP server",
        "List MCP servers",
        "Remove MCP server",
        "Sync (reconcile configs)",
        "← Back",
    ];

    let choice = Select::new("MCPs >", options).prompt()?;

    match choice {
        "Add MCP server" => mcps::add::add_mcp(config, None)?,
        "List MCP servers" => mcps::list::list_mcps(config)?,
        "Remove MCP server" => mcps::remove::remove_mcp(config, None)?,
        "Sync (reconcile configs)" => mcps::sync::sync_mcps(config)?,
        "← Back" => {}
        _ => unreachable!(),
    }

    Ok(())
}

fn marketplaces_menu(config: &mut Config) -> Result<()> {
    let options = vec![
        "Browse skills",
        "Add marketplace",
        "Update marketplace cache",
        "List marketplaces",
        "Remove marketplace",
        "← Back",
    ];

    let choice = Select::new("Marketplaces >", options).prompt()?;

    match choice {
        "Browse skills" => market::browse::browse_marketplace(config, None)?,
        "Add marketplace" => {
            let repo = inquire::Text::new("GitHub repo (owner/repo):").prompt()?;
            market::manage::add_marketplace(config, &repo)?;
        }
        "Update marketplace cache" => {
            market::cache::update_all_marketplaces(&config.marketplaces)?
        }
        "List marketplaces" => market::manage::list_marketplaces(config)?,
        "Remove marketplace" => market::manage::remove_marketplace(config, None)?,
        "← Back" => {}
        _ => unreachable!(),
    }

    Ok(())
}

fn agents_menu(config: &mut Config) -> Result<()> {
    let options = vec![
        "List agents",
        "Add custom agent",
        "Remove custom agent",
        "← Back",
    ];

    let choice = Select::new("Agents >", options).prompt()?;

    match choice {
        "List agents" => {
            let dim = console::Style::new().dim();
            let green = console::Style::new().green();
            println!("{:<12} {:<16} {:<30} {}", "ID", "Name", "Skills Path", "MCP Config");
            println!(
                "{:<12} {:<16} {:<30} {}",
                "─".repeat(10),
                "─".repeat(14),
                "─".repeat(28),
                "─".repeat(30)
            );
            for (id, agent) in &config.agents {
                let enabled = if agent.enabled {
                    green.apply_to("●")
                } else {
                    dim.apply_to("○")
                };
                println!(
                    "{} {:<10} {:<16} {:<30} {}",
                    enabled,
                    id,
                    agent.name,
                    agent.skills_path,
                    agent.mcp_config.as_deref().unwrap_or("—")
                );
            }
        }
        "Add custom agent" => {
            let id = inquire::Text::new("Short name (e.g., cursor):").prompt()?;
            let name = inquire::Text::new("Display name:").prompt()?;
            let skills_path = inquire::Text::new("Skills path (e.g., ~/.cursor/skills):").prompt()?;
            let mcp_config = inquire::Text::new("MCP config path (empty to skip):").prompt()?;
            let mcp_config = if mcp_config.is_empty() {
                None
            } else {
                Some(mcp_config)
            };
            let mcp_format = if mcp_config.is_some() {
                let fmt = Select::new("MCP config format:", vec!["json", "toml"]).prompt()?;
                match fmt {
                    "toml" => crate::agents::McpFormat::Toml,
                    _ => crate::agents::McpFormat::Json,
                }
            } else {
                crate::agents::McpFormat::Json
            };
            let mcp_key = if mcp_config.is_some() {
                Some(inquire::Text::new("MCP key in config (e.g., mcpServers):").prompt()?)
            } else {
                None
            };

            config.agents.insert(
                id,
                crate::agents::AgentConfig {
                    name,
                    skills_path,
                    mcp_config,
                    mcp_format,
                    mcp_key,
                    enabled: true,
                },
            );
            config.save()?;
            println!("Agent added.");
        }
        "Remove custom agent" => {
            let builtins = ["claude", "codex", "gemini"];
            let custom: Vec<String> = config
                .agents
                .keys()
                .filter(|k| !builtins.contains(&k.as_str()))
                .cloned()
                .collect();
            if custom.is_empty() {
                println!("No custom agents to remove.");
            } else {
                let selected = Select::new("Select agent to remove:", custom).prompt()?;
                config.agents.remove(&selected);
                config.save()?;
                println!("Removed '{}'.", selected);
            }
        }
        "← Back" => {}
        _ => unreachable!(),
    }

    Ok(())
}
```

- [ ] **Step 2: Wire wizard into main.rs**

Add `mod wizard;` and update the `None` branch:

```rust
None => {
    wizard::run_wizard()?;
    Ok(())
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cd /home/julius/00_projects/skillr && cargo build`
Expected: Compiles with no errors.

- [ ] **Step 4: Test the wizard interactively**

Run: `cd /home/julius/00_projects/skillr && cargo run`
Expected: Shows the main menu with Skills/MCPs/Marketplaces/Agents/Sync All/Exit options. Navigate menus, verify "List agents" shows the 3 defaults.

- [ ] **Step 5: Test CLI subcommands**

Run: `cd /home/julius/00_projects/skillr && cargo run -- skills list`
Expected: Shows skill table (or "No skills installed" if none exist).

Run: `cd /home/julius/00_projects/skillr && cargo run -- mcps list`
Expected: Shows MCP table (or "No MCP servers configured").

- [ ] **Step 6: Commit**

```bash
cd /home/julius/00_projects/skillr
git add src/wizard.rs src/main.rs
git commit -m "feat: add interactive wizard with full menu navigation"
```

---

### Task 8: End-to-End Testing & Polish

**Files:**
- Modify: `src/main.rs` (any final wiring fixes)
- Modify: Various files (compilation fixes from integration)

- [ ] **Step 1: Full build check**

Run: `cd /home/julius/00_projects/skillr && cargo build 2>&1`
Expected: Compiles with no errors. Fix any issues found.

- [ ] **Step 2: Run clippy**

Run: `cd /home/julius/00_projects/skillr && cargo clippy 2>&1`
Expected: No warnings. Fix any clippy suggestions.

- [ ] **Step 3: Test skill sync with real agent directories**

Run: `cd /home/julius/00_projects/skillr && cargo run -- skills list`
Expected: Detects existing skills in `~/.agents/skills/`, `~/.claude/skills/`, `~/.codex/skills/`, `~/.gemini/skills/` and shows their status.

- [ ] **Step 4: Test MCP sync against real configs**

Run: `cd /home/julius/00_projects/skillr && cargo run -- mcps sync`
Expected: Reads MCPs from `~/.codex/config.toml` and `~/.gemini/settings.json`, offers to import them to central registry.

- [ ] **Step 5: Test marketplace add + browse**

Run: `cd /home/julius/00_projects/skillr && cargo run -- market add anthropics/skills`
Expected: Clones repo, saves to config.

Run: `cd /home/julius/00_projects/skillr && cargo run -- market browse`
Expected: Shows skills from the repo with fuzzy select.

- [ ] **Step 6: Install the binary**

Run: `cd /home/julius/00_projects/skillr && cargo install --path .`
Expected: Installs `skillr` to `~/.cargo/bin/skillr`.

- [ ] **Step 7: Commit final polish**

```bash
cd /home/julius/00_projects/skillr
git add -A
git commit -m "chore: clippy fixes and integration polish"
```

---

## Summary

| Task | Description | Est. Steps |
|------|-------------|-----------|
| 1 | Project scaffold, config, agents, util | 9 |
| 2 | Lockfile (.skill-lock.json) | 4 |
| 3 | Skills list & bidirectional sync | 7 |
| 4 | Skills install (GitHub + local) & remove | 4 |
| 5 | MCP registry, config patching, add/list/remove/sync | 10 |
| 6 | Marketplace browse, add, update, manage | 7 |
| 7 | Interactive wizard | 6 |
| 8 | End-to-end testing & polish | 7 |
