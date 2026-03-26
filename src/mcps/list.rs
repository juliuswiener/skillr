use std::collections::BTreeSet;

use anyhow::Result;
use console::Style;

use crate::config::Config;
use crate::mcps::patch::read_agent_mcps;
use crate::mcps::registry::McpRegistry;

/// Print a table of all MCP servers and their status per agent.
pub fn list_mcps(config: &Config) -> Result<()> {
    let registry = McpRegistry::load()?;
    let enabled = config.enabled_agents();

    // Collect all unique MCP names from registry + agent configs.
    let mut all_names = BTreeSet::new();
    for name in registry.servers.keys() {
        all_names.insert(name.clone());
    }

    // Read each agent's MCPs.
    let mut agent_mcps = Vec::new();
    for (id, agent) in &enabled {
        let mcps = read_agent_mcps(agent)?;
        for name in mcps.keys() {
            all_names.insert(name.clone());
        }
        agent_mcps.push((id.to_string(), mcps));
    }

    if all_names.is_empty() {
        println!("No MCP servers registered.");
        return Ok(());
    }

    let green = Style::new().green();
    let dim = Style::new().dim();

    // Compute column widths.
    let name_width = all_names.iter().map(|n| n.len()).max().unwrap_or(10).max(10);
    let command_width = {
        let max_cmd = all_names
            .iter()
            .map(|n| {
                registry
                    .servers
                    .get(n)
                    .map(|e| e.command.len())
                    .unwrap_or(0)
            })
            .max()
            .unwrap_or(7);
        max_cmd.max(7)
    };

    let agent_ids: Vec<&String> = enabled.keys().copied().collect();
    let agent_widths: Vec<usize> = agent_ids.iter().map(|id| id.len().max(3)).collect();

    // Header.
    print!(
        "{:nw$}  {:cw$}",
        "MCP Server",
        "Command",
        nw = name_width,
        cw = command_width
    );
    for (i, id) in agent_ids.iter().enumerate() {
        print!("  {:>width$}", id, width = agent_widths[i]);
    }
    println!();

    // Separator.
    print!(
        "{:-<nw$}  {:-<cw$}",
        "",
        "",
        nw = name_width,
        cw = command_width
    );
    for (i, _) in agent_ids.iter().enumerate() {
        print!("  {:-<width$}", "", width = agent_widths[i]);
    }
    println!();

    // Rows.
    for mcp_name in &all_names {
        let command = registry
            .servers
            .get(mcp_name)
            .map(|e| e.command.as_str())
            .unwrap_or("");

        print!(
            "{:nw$}  {:cw$}",
            mcp_name,
            command,
            nw = name_width,
            cw = command_width
        );

        for (i, (_agent_id, agent_mcp_map)) in agent_mcps.iter().enumerate() {
            let present = agent_mcp_map.contains_key(mcp_name);
            let cell = if present {
                format!("{}", green.apply_to("\u{2713}"))
            } else {
                format!("{}", dim.apply_to("\u{2717}"))
            };
            print!("  {:>width$}", cell, width = agent_widths[i]);
        }
        println!();
    }

    Ok(())
}
