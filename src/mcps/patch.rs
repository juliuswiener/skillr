use std::collections::BTreeMap;
use std::fs;

use anyhow::{Context, Result};
use serde_json::Value as JsonValue;

use crate::agents::{AgentConfig, McpFormat};
use crate::mcps::registry::McpEntry;

/// Read all MCP server entries from an agent's config file.
pub fn read_agent_mcps(agent: &AgentConfig) -> Result<BTreeMap<String, McpEntry>> {
    let Some(path) = agent.mcp_config_expanded() else {
        return Ok(BTreeMap::new());
    };
    if !path.exists() {
        return Ok(BTreeMap::new());
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;

    match agent.mcp_format {
        McpFormat::Json => read_json_mcps(&content, agent),
        McpFormat::Toml => read_toml_mcps(&content, agent),
    }
}

fn read_json_mcps(content: &str, agent: &AgentConfig) -> Result<BTreeMap<String, McpEntry>> {
    let root: JsonValue = serde_json::from_str(content).context("failed to parse JSON MCP config")?;
    let key = agent.mcp_key.as_deref().unwrap_or("mcpServers");

    let Some(servers_obj) = root.get(key).and_then(|v| v.as_object()) else {
        return Ok(BTreeMap::new());
    };

    let mut result = BTreeMap::new();
    for (name, val) in servers_obj {
        let Some(obj) = val.as_object() else { continue };
        let command = obj
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let args = obj
            .get("args")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let env = obj
            .get("env")
            .and_then(|v| v.as_object())
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();

        result.insert(
            name.clone(),
            McpEntry {
                command,
                args,
                env,
                agents: Vec::new(),
            },
        );
    }
    Ok(result)
}

fn read_toml_mcps(content: &str, agent: &AgentConfig) -> Result<BTreeMap<String, McpEntry>> {
    let root: toml::Value = toml::from_str(content).context("failed to parse TOML MCP config")?;
    let key = agent.mcp_key.as_deref().unwrap_or("mcp_servers");

    let Some(servers_table) = root.get(key).and_then(|v| v.as_table()) else {
        return Ok(BTreeMap::new());
    };

    let mut result = BTreeMap::new();
    for (name, val) in servers_table {
        let Some(tbl) = val.as_table() else { continue };
        // Skip sub-entries that have no "command" — they're env tables.
        let Some(command) = tbl.get("command").and_then(|v| v.as_str()) else {
            continue;
        };
        let args = tbl
            .get("args")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let env = tbl
            .get("env")
            .and_then(|v| v.as_table())
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();

        result.insert(
            name.clone(),
            McpEntry {
                command: command.to_string(),
                args,
                env,
                agents: Vec::new(),
            },
        );
    }
    Ok(result)
}

/// Write (or overwrite) a single MCP server entry in an agent's config file.
pub fn write_agent_mcp(agent: &AgentConfig, name: &str, entry: &McpEntry) -> Result<()> {
    let Some(path) = agent.mcp_config_expanded() else {
        anyhow::bail!("agent '{}' has no mcp_config path", agent.name);
    };

    // Create parent directory if needed.
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }

    // Backup original file if it exists.
    if path.exists() {
        let bak = path.with_extension(
            path.extension()
                .map(|e| format!("{}.bak", e.to_string_lossy()))
                .unwrap_or_else(|| "bak".to_string()),
        );
        fs::copy(&path, &bak)
            .with_context(|| format!("failed to backup {}", path.display()))?;
    }

    match agent.mcp_format {
        McpFormat::Json => write_json_mcp(agent, name, entry, &path),
        McpFormat::Toml => write_toml_mcp(agent, name, entry, &path),
    }
}

fn write_json_mcp(
    agent: &AgentConfig,
    name: &str,
    entry: &McpEntry,
    path: &std::path::Path,
) -> Result<()> {
    let key = agent.mcp_key.as_deref().unwrap_or("mcpServers");

    let mut root: JsonValue = if path.exists() {
        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        serde_json::from_str(&content).context("failed to parse JSON")?
    } else {
        serde_json::json!({})
    };

    let servers = root
        .as_object_mut()
        .context("root is not an object")?
        .entry(key.to_string())
        .or_insert_with(|| serde_json::json!({}));

    let server_obj = serde_json::json!({
        "command": entry.command,
        "args": entry.args,
        "env": entry.env,
    });

    servers
        .as_object_mut()
        .context("servers section is not an object")?
        .insert(name.to_string(), server_obj);

    let output = serde_json::to_string_pretty(&root).context("failed to serialize JSON")?;
    fs::write(path, output.as_bytes())
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn write_toml_mcp(
    agent: &AgentConfig,
    name: &str,
    entry: &McpEntry,
    path: &std::path::Path,
) -> Result<()> {
    let key = agent.mcp_key.as_deref().unwrap_or("mcp_servers");

    let content = if path.exists() {
        fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?
    } else {
        String::new()
    };

    let mut doc: toml_edit::DocumentMut = content
        .parse()
        .context("failed to parse TOML document")?;

    // Ensure the mcp_servers table exists.
    if !doc.contains_key(key) {
        doc[key] = toml_edit::Item::Table(toml_edit::Table::new());
    }

    // Build the server sub-table.
    let mut server_table = toml_edit::Table::new();
    server_table.insert("command", toml_edit::value(&entry.command));

    if !entry.args.is_empty() {
        let mut args_arr = toml_edit::Array::new();
        for arg in &entry.args {
            args_arr.push(arg.as_str());
        }
        server_table.insert("args", toml_edit::value(args_arr));
    }

    if !entry.env.is_empty() {
        let mut env_table = toml_edit::InlineTable::new();
        for (k, v) in &entry.env {
            env_table.insert(k, v.as_str().into());
        }
        server_table.insert("env", toml_edit::value(env_table));
    }

    doc[key][name] = toml_edit::Item::Table(server_table);

    fs::write(path, doc.to_string().as_bytes())
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

/// Remove a single MCP server entry from an agent's config file.
pub fn remove_agent_mcp(agent: &AgentConfig, name: &str) -> Result<()> {
    let Some(path) = agent.mcp_config_expanded() else {
        return Ok(());
    };
    if !path.exists() {
        return Ok(());
    }

    // Backup original.
    let bak = path.with_extension(
        path.extension()
            .map(|e| format!("{}.bak", e.to_string_lossy()))
            .unwrap_or_else(|| "bak".to_string()),
    );
    fs::copy(&path, &bak)
        .with_context(|| format!("failed to backup {}", path.display()))?;

    match agent.mcp_format {
        McpFormat::Json => remove_json_mcp(agent, name, &path),
        McpFormat::Toml => remove_toml_mcp(agent, name, &path),
    }
}

fn remove_json_mcp(agent: &AgentConfig, name: &str, path: &std::path::Path) -> Result<()> {
    let key = agent.mcp_key.as_deref().unwrap_or("mcpServers");
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let mut root: JsonValue = serde_json::from_str(&content).context("failed to parse JSON")?;

    if let Some(servers) = root
        .as_object_mut()
        .and_then(|o| o.get_mut(key))
        .and_then(|v| v.as_object_mut())
    {
        servers.remove(name);
    }

    let output = serde_json::to_string_pretty(&root).context("failed to serialize JSON")?;
    fs::write(path, output.as_bytes())
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn remove_toml_mcp(agent: &AgentConfig, name: &str, path: &std::path::Path) -> Result<()> {
    let key = agent.mcp_key.as_deref().unwrap_or("mcp_servers");
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let mut doc: toml_edit::DocumentMut = content
        .parse()
        .context("failed to parse TOML document")?;

    if let Some(servers) = doc.get_mut(key).and_then(|v| v.as_table_mut()) {
        servers.remove(name);
    }

    fs::write(path, doc.to_string().as_bytes())
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}
