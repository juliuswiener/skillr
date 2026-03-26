use anyhow::Result;
use console::Style;
use inquire::Confirm;

use crate::config::Config;
use crate::mcps::patch::{read_agent_mcps, write_agent_mcp};
use crate::mcps::registry::{McpEntry, McpRegistry};

/// Bidirectional sync between the central MCP registry and agent configs.
pub fn sync_mcps(config: &Config) -> Result<()> {
    let green = Style::new().green();
    let bold = Style::new().bold();

    let mut registry = McpRegistry::load()?;
    let enabled = config.enabled_agents();
    let mut registry_changed = false;

    println!(
        "{}",
        bold.apply_to("Syncing MCP server configurations...")
    );

    for (agent_id, agent) in &enabled {
        let agent_mcps = read_agent_mcps(agent)?;
        let central_for_agent = registry.servers_for_agent(agent_id);

        // Central MCPs missing from agent config -> offer to push.
        for name in central_for_agent.keys() {
            if !agent_mcps.contains_key(name) {
                let prompt = format!(
                    "  Push '{}' to {} config?",
                    name, agent.name
                );
                let push = Confirm::new(&prompt)
                    .with_default(true)
                    .prompt()
                    .unwrap_or(false);

                if push {
                    // Get owned entry from registry.
                    if let Some(entry) = registry.servers.get(name) {
                        write_agent_mcp(agent, name, entry)?;
                        println!(
                            "    {} Pushed '{}' to {}.",
                            green.apply_to("\u{2713}"),
                            name,
                            agent.name
                        );
                    }
                }
            }
        }

        // Agent MCPs not in central -> offer to import.
        for (name, entry) in &agent_mcps {
            if !registry.servers.contains_key(name) {
                let prompt = format!(
                    "  Import '{}' from {} into central registry?",
                    name, agent.name
                );
                let import = Confirm::new(&prompt)
                    .with_default(true)
                    .prompt()
                    .unwrap_or(false);

                if import {
                    let new_entry = McpEntry {
                        command: entry.command.clone(),
                        args: entry.args.clone(),
                        env: entry.env.clone(),
                        agents: vec![agent_id.to_string()],
                    };
                    registry.servers.insert(name.clone(), new_entry);
                    registry_changed = true;
                    println!(
                        "    {} Imported '{}' from {}.",
                        green.apply_to("\u{2713}"),
                        name,
                        agent.name
                    );
                }
            }
        }
    }

    if registry_changed {
        registry.save()?;
        println!(
            "\n{} Registry updated.",
            green.apply_to("\u{2713}")
        );
    }

    println!("{}", green.apply_to("MCP sync complete."));
    Ok(())
}
