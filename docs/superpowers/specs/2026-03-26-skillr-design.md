# skillr — Unified AI Agent Skill & MCP Manager

## Overview

A Rust CLI with interactive wizard-style menus (`inquire`) that manages skills and MCP servers across AI coding agents (Claude Code, Codex, Gemini CLI, and custom agents). It uses `~/.agents/` as the single source of truth with symlinks into each agent's native directory.

## Goals

1. One command (`skillr`) to install, remove, sync, and browse skills across all agents
2. Bidirectional sync: detect new skills in agent dirs, centralize them, symlink back
3. Central MCP registry that patches each agent's native config format
4. Marketplace browsing with fuzzy search from GitHub skill repos
5. Extensible agent paths for future agents

## Non-Goals

- Replacing agent-specific plugin systems (Claude plugins, Gemini extensions)
- Managing agent configs beyond skills and MCPs
- Running as a daemon or background service
- Web UI

---

## Architecture

### Directory Layout

```
~/.agents/                          # Central source of truth
├── config.toml                     # Agent paths, marketplaces, settings
├── mcps.toml                       # Central MCP registry
├── .skill-lock.json                # Skill provenance tracking (compatible with playbooks)
├── cache/                          # Cloned marketplace repos
│   ├── anthropics-skills/
│   └── openclaw-skills/
└── skills/                         # Canonical skill directories
    ├── prompt-writer/
    │   ├── SKILL.md
    │   └── references/
    ├── flutter/
    │   └── SKILL.md
    └── ...
```

### Agent Paths (Defaults)

| Agent | Skills Path | MCP Config File | MCP Format |
|-------|------------|----------------|------------|
| Claude Code | `~/.claude/skills/` | `~/.claude/settings.json` | JSON `mcpServers` |
| Codex | `~/.codex/skills/` | `~/.codex/config.toml` | TOML `[mcp_servers.*]` |
| Gemini CLI | `~/.gemini/skills/` | `~/.gemini/settings.json` | JSON `mcpServers` |
| Custom | User-defined in config.toml | User-defined | JSON or TOML |

All paths are configurable in `~/.agents/config.toml`.

### Symlink Strategy

- Skills always live as real directories under `~/.agents/skills/<name>/`
- Agent skill dirs contain symlinks: `~/.claude/skills/foo -> ../../.agents/skills/foo`
- Relative symlinks are used so they survive home-dir renames

---

## Config Files

### `~/.agents/config.toml`

```toml
[agents.claude]
name = "Claude Code"
skills_path = "~/.claude/skills"
mcp_config = "~/.claude/settings.json"
mcp_format = "json"           # "json" or "toml"
mcp_key = "mcpServers"        # JSON path to MCP object
enabled = true

[agents.codex]
name = "Codex"
skills_path = "~/.codex/skills"
mcp_config = "~/.codex/config.toml"
mcp_format = "toml"
mcp_key = "mcp_servers"
enabled = true

[agents.gemini]
name = "Gemini CLI"
skills_path = "~/.gemini/skills"
mcp_config = "~/.gemini/settings.json"
mcp_format = "json"
mcp_key = "mcpServers"
enabled = true

[[marketplaces]]
name = "anthropics/skills"
url = "https://github.com/anthropics/skills"

[[marketplaces]]
name = "openclaw/skills"
url = "https://github.com/openclaw/skills"
```

### `~/.agents/mcps.toml`

```toml
[context7]
command = "npx"
args = ["-y", "@upstash/context7-mcp"]
agents = ["claude", "codex", "gemini"]

[filesystem-with-morph]
command = "npx"
args = ["-y", "@morph-llm/morph-fast-apply"]
agents = ["claude", "gemini"]

[filesystem-with-morph.env]
ALL_TOOLS = "true"
MORPH_API_KEY = "sk-..."
```

### `~/.agents/.skill-lock.json`

Existing format, maintained for compatibility with playbooks:

```json
{
  "version": 3,
  "skills": {
    "prompt-writer": {
      "source": "openclaw/skills",
      "sourceType": "github",
      "sourceUrl": "https://github.com/openclaw/skills.git",
      "skillPath": "skills/prompt-writer/SKILL.md",
      "skillFolderHash": "abc123...",
      "installedAt": "2026-03-26T12:00:00Z",
      "updatedAt": "2026-03-26T12:00:00Z"
    }
  }
}
```

---

## Wizard Flow

Running `skillr` with no arguments launches the interactive wizard:

```
? What would you like to do?
> Skills
  MCPs
  Marketplaces
  Agents
  Sync All
  Settings
```

### Skills Menu

```
? Skills >
> Install skill
  List installed
  Remove skill
  Sync (detect drift)
```

**Install skill** — multi-source:
1. "From marketplace" → fuzzy-select from cached marketplace skills
2. "From GitHub" → enter `owner/repo` or full URL, select skill(s) from repo
3. "From local path" → enter path to a skill directory

After selecting a skill, multi-select which agents to install to. Creates symlinks in selected agent dirs.

**List installed** — table showing:
```
Skill            Source                  claude  codex  gemini
prompt-writer    openclaw/skills         ✓ (sym) ✓ (sym) ✓ (sym)
flutter          openclaw/skills         ✓ (sym) ✗       ✗
my-custom-skill  local                   ✓ (sym) ✓ (sym) ✗
orphan-skill     —                       ✗       ✓ (dir) ✗      ← not centralized
```

**Sync** — bidirectional reconciliation:
1. Scan each agent's skills dir for non-symlinked skill directories
2. If skill not in `~/.agents/skills/`: copy it there, replace with symlink
3. If skill in central but missing from an agent that should have it: recreate symlink
4. Report broken symlinks and offer to fix

### MCPs Menu

```
? MCPs >
> Add MCP server
  List MCP servers
  Remove MCP server
  Sync (reconcile configs)
  Import from agent
```

**Add MCP** — wizard prompts:
1. Name (e.g., `context7`)
2. Command (e.g., `npx`)
3. Args (e.g., `-y @upstash/context7-mcp`)
4. Environment variables (optional, key=value pairs)
5. Which agents to enable for (multi-select)

Writes to `~/.agents/mcps.toml` then patches each selected agent's config.

**Import from agent** — reads an agent's MCP config file, shows MCPs not in central registry, offers to import them.

**Sync** — compares central registry against each agent's config:
- MCPs in central but missing from agent config → offer to add
- MCPs in agent config but not in central → offer to import or ignore
- MCPs with differing configs → show diff, offer to reconcile

### Marketplaces Menu

```
? Marketplaces >
> Browse skills
  Add marketplace
  Update marketplace cache
  List marketplaces
  Remove marketplace
```

**Browse** — fuzzy search across all cached marketplace skills. Shows name, description (from SKILL.md frontmatter). Select to install.

**Add marketplace** — enter GitHub `owner/repo` URL. Clones to `~/.agents/cache/<name>/`. Scans for SKILL.md files in standard locations (`skills/`, root, recursive fallback).

**Update** — `git pull` on all cached marketplace repos.

### Agents Menu

```
? Agents >
> List agents
  Add custom agent
  Edit agent config
  Remove custom agent
```

**Add custom agent** — prompts for:
1. Short name (e.g., `cursor`)
2. Display name
3. Skills path
4. MCP config path (optional)
5. MCP format (json/toml)
6. MCP key path

### Sync All

Runs both skill sync and MCP sync in one pass. This is the "make everything consistent" command.

---

## CLI Interface

Besides the wizard, direct commands for scripting:

```bash
skillr                              # Launch wizard
skillr sync                         # Sync all (skills + MCPs)
skillr skills list                  # List skills
skillr skills install <source>      # Install from GitHub/path
skillr skills remove <name>         # Remove skill
skillr mcps list                    # List MCPs
skillr mcps add <name>              # Add MCP interactively
skillr mcps sync                    # Sync MCP configs
skillr market browse [query]        # Browse marketplace skills
skillr market add <repo>            # Add marketplace
skillr market update                # Update marketplace caches
```

---

## Data Flow

### Skill Installation (from marketplace)

```
1. User selects skill from marketplace browse
2. Copy skill directory from cache → ~/.agents/skills/<name>/
3. Update .skill-lock.json with source metadata
4. User selects target agents
5. For each agent: create relative symlink in agent's skills dir
```

### Bidirectional Skill Sync

```
For each agent:
  For each entry in agent's skills dir:
    If symlink pointing to ~/.agents/skills/ → OK (skip)
    If symlink broken → report, offer delete
    If real directory:
      If ~/.agents/skills/<name> exists → conflict (ask user)
      Else → move to ~/.agents/skills/<name>, create symlink
  For each skill in ~/.agents/skills/:
    If no symlink in agent dir → offer to create one
```

### MCP Sync

```
For each agent:
  Read agent's native MCP config
  Compare against central mcps.toml (filtered by agent)
  Show diffs:
    - In central, missing from agent → "push to agent?"
    - In agent, missing from central → "import to central?"
    - Config mismatch → show diff, "which to keep?"
  Apply chosen changes to both central and agent configs
```

### MCP Config Patching

When writing MCPs to agent configs:

**Claude/Gemini (JSON)**: Read file → parse JSON → update `mcpServers` key → write back with formatting preserved (use `serde_json` with pretty print)

**Codex (TOML)**: Read file → parse TOML → update `mcp_servers` section → write back (use `toml_edit` to preserve formatting/comments)

---

## Crate Dependencies

```toml
[dependencies]
inquire = "0.7"          # Interactive prompts
serde = { version = "1", features = ["derive"] }
serde_json = "1"         # JSON config parsing
toml = "0.8"             # TOML config parsing
toml_edit = "0.22"       # TOML editing preserving formatting
clap = { version = "4", features = ["derive"] }  # CLI args
walkdir = "2"            # Directory traversal
console = "0.15"         # Terminal colors/formatting
indicatif = "0.17"       # Progress bars
dirs = "6"               # Home dir resolution
git2 = "0.20"            # Git operations (clone, pull)
sha2 = "0.10"            # Folder hashing
chrono = "0.4"           # Timestamps
anyhow = "1"             # Error handling
```

---

## Error Handling

- All operations are transactional where possible: copy then symlink, not symlink then copy
- Broken symlinks are detected and reported, never silently ignored
- Config file writes use write-to-temp-then-rename for atomicity
- Agent configs are backed up before patching (`.bak` suffix)
- Permission errors on agent dirs are reported clearly ("cannot write to ~/.claude/skills/ — check permissions")

---

## Future Extensions

- Plugin/hook management (Claude hooks, Gemini extensions)
- Skill versioning and updates (check upstream hashes)
- `skillr export` — generate a portable playbook file
- `skillr import` — apply a playbook to set up a fresh machine
- Shell completions (fish, bash, zsh)
