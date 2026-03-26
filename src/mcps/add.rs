use std::collections::BTreeMap;

use anyhow::Result;
use console::Style;
use inquire::{MultiSelect, Text};

use crate::config::Config;
use crate::mcps::patch::write_agent_mcp;
use crate::mcps::registry::{McpEntry, McpRegistry};

/// Interactively add an MCP server to the central registry and patch agent configs.
pub fn add_mcp(config: &Config, name: Option<&str>) -> Result<()> {
    let green = Style::new().green();

    let name = match name {
        Some(n) => n.to_string(),
        None => Text::new("MCP server name:")
            .prompt()
            .map_err(|e| anyhow::anyhow!("{}", e))?,
    };

    let command = Text::new("Command:")
        .prompt()
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let args_str = Text::new("Arguments (space-separated, or empty):")
        .with_default("")
        .prompt()
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let args: Vec<String> = if args_str.trim().is_empty() {
        Vec::new()
    } else {
        args_str.split_whitespace().map(|s| s.to_string()).collect()
    };

    let mut env = BTreeMap::new();
    println!("Environment variables (KEY=VALUE, empty line to finish):");
    loop {
        let line = Text::new("  env>")
            .with_default("")
            .prompt()
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        let line = line.trim().to_string();
        if line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once('=') {
            env.insert(k.trim().to_string(), v.trim().to_string());
        } else {
            println!("  (expected KEY=VALUE format, skipping)");
        }
    }

    // Select agents.
    let enabled = config.enabled_agents();
    let agent_ids: Vec<String> = enabled.keys().map(|k| k.to_string()).collect();

    let selected_agents = if agent_ids.is_empty() {
        Vec::new()
    } else {
        // All pre-selected.
        let defaults: Vec<usize> = (0..agent_ids.len()).collect();
        MultiSelect::new("Select agents:", agent_ids.clone())
            .with_default(&defaults)
            .prompt()
            .map_err(|e| anyhow::anyhow!("{}", e))?
    };

    let entry = McpEntry {
        command,
        args,
        env,
        agents: selected_agents.clone(),
    };

    // Save to central registry.
    let mut registry = McpRegistry::load()?;
    registry.servers.insert(name.clone(), entry.clone());
    registry.save()?;
    println!(
        "{} Registered '{}' in central registry.",
        green.apply_to("✓"),
        name
    );

    // Patch each selected agent's config.
    for agent_id in &selected_agents {
        if let Some(agent) = config.agents.get(agent_id) {
            write_agent_mcp(agent, &name, &entry)?;
            println!(
                "{} Patched {} config.",
                green.apply_to("✓"),
                agent.name
            );
        }
    }

    Ok(())
}
