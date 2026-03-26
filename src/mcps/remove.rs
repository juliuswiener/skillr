use anyhow::Result;
use console::Style;
use inquire::{Confirm, Select};

use crate::config::Config;
use crate::mcps::patch::remove_agent_mcp;
use crate::mcps::registry::McpRegistry;

/// Interactively remove an MCP server from the central registry and all agent configs.
pub fn remove_mcp(config: &Config, name: Option<&str>) -> Result<()> {
    let green = Style::new().green();
    let mut registry = McpRegistry::load()?;

    if registry.servers.is_empty() {
        println!("No MCP servers registered.");
        return Ok(());
    }

    let name = match name {
        Some(n) => n.to_string(),
        None => {
            let options: Vec<String> = registry.servers.keys().cloned().collect();
            Select::new("Select MCP server to remove:", options)
                .prompt()
                .map_err(|e| anyhow::anyhow!("{}", e))?
        }
    };

    if !registry.servers.contains_key(&name) {
        anyhow::bail!("MCP server '{}' not found in registry", name);
    }

    let confirmed = Confirm::new(&format!("Remove MCP server '{}'?", name))
        .with_default(false)
        .prompt()
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    if !confirmed {
        println!("Aborted.");
        return Ok(());
    }

    // Remove from each agent's config.
    for agent in config.agents.values() {
        if agent.enabled {
            remove_agent_mcp(agent, &name)?;
        }
    }
    println!(
        "{} Removed '{}' from agent configs.",
        green.apply_to("\u{2713}"),
        name
    );

    // Remove from central registry.
    registry.servers.remove(&name);
    registry.save()?;
    println!(
        "{} Removed '{}' from central registry.",
        green.apply_to("\u{2713}"),
        name
    );

    Ok(())
}
