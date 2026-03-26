use anyhow::Result;
use console::Style;
use inquire::Select;

use crate::agents::{AgentConfig, McpFormat};
use crate::config::Config;
use crate::{market, mcps, skills};

const BUILTIN_AGENTS: &[&str] = &["claude", "codex", "gemini"];

pub fn run_wizard() -> Result<()> {
    loop {
        let config = Config::load()?;

        let choices = vec![
            "Skills",
            "MCPs",
            "Marketplaces",
            "Agents",
            "Sync All",
            "Exit",
        ];

        let selection = Select::new("What would you like to do?", choices).prompt();

        match selection {
            Ok("Skills") => skills_menu(&config)?,
            Ok("MCPs") => mcps_menu(&config)?,
            Ok("Marketplaces") => {
                let mut config = config;
                marketplaces_menu(&mut config)?;
            }
            Ok("Agents") => {
                let mut config = config;
                agents_menu(&mut config)?;
            }
            Ok("Sync All") => {
                skills::sync::sync_skills(&config)?;
                mcps::sync::sync_mcps(&config)?;
            }
            Ok("Exit") | Err(_) => break,
            _ => {}
        }
    }

    Ok(())
}

fn skills_menu(config: &Config) -> Result<()> {
    let choices = vec![
        "Install skill",
        "List installed",
        "Remove skill",
        "Sync (detect drift)",
        "\u{2190} Back",
    ];

    let selection = Select::new("Skills", choices).prompt();

    match selection {
        Ok("Install skill") => skills::install::install_skill(config, None)?,
        Ok("List installed") => skills::list::print_skills_table(config)?,
        Ok("Remove skill") => skills::remove::remove_skill(config, None)?,
        Ok("Sync (detect drift)") => skills::sync::sync_skills(config)?,
        Ok("\u{2190} Back") | Err(_) => {}
        _ => {}
    }

    Ok(())
}

fn mcps_menu(config: &Config) -> Result<()> {
    let choices = vec![
        "Add MCP server",
        "List MCP servers",
        "Remove MCP server",
        "Sync (reconcile configs)",
        "\u{2190} Back",
    ];

    let selection = Select::new("MCPs", choices).prompt();

    match selection {
        Ok("Add MCP server") => mcps::add::add_mcp(config, None)?,
        Ok("List MCP servers") => mcps::list::list_mcps(config)?,
        Ok("Remove MCP server") => mcps::remove::remove_mcp(config, None)?,
        Ok("Sync (reconcile configs)") => mcps::sync::sync_mcps(config)?,
        Ok("\u{2190} Back") | Err(_) => {}
        _ => {}
    }

    Ok(())
}

fn marketplaces_menu(config: &mut Config) -> Result<()> {
    let choices = vec![
        "Browse skills",
        "Add marketplace",
        "Update marketplace cache",
        "List marketplaces",
        "Remove marketplace",
        "\u{2190} Back",
    ];

    let selection = Select::new("Marketplaces", choices).prompt();

    match selection {
        Ok("Browse skills") => market::browse::browse_marketplace(config, None)?,
        Ok("Add marketplace") => {
            let repo = inquire::Text::new("Marketplace git repo URL:").prompt()?;
            market::manage::add_marketplace(config, &repo)?;
        }
        Ok("Update marketplace cache") => {
            market::cache::update_all_marketplaces(&config.marketplaces)?;
        }
        Ok("List marketplaces") => market::manage::list_marketplaces(config)?,
        Ok("Remove marketplace") => market::manage::remove_marketplace(config, None)?,
        Ok("\u{2190} Back") | Err(_) => {}
        _ => {}
    }

    Ok(())
}

fn agents_menu(config: &mut Config) -> Result<()> {
    let choices = vec![
        "List agents",
        "Add custom agent",
        "Remove custom agent",
        "\u{2190} Back",
    ];

    let selection = Select::new("Agents", choices).prompt();

    match selection {
        Ok("List agents") => print_agents_table(config),
        Ok("Add custom agent") => add_custom_agent(config)?,
        Ok("Remove custom agent") => remove_custom_agent(config)?,
        Ok("\u{2190} Back") | Err(_) => {}
        _ => {}
    }

    Ok(())
}

fn print_agents_table(config: &Config) {
    let green = Style::new().green();
    let dim = Style::new().dim();

    println!(
        "{:<12} {:<15} {:<30} {:<30}",
        "ID", "Name", "Skills Path", "MCP Config"
    );
    println!("{}", "-".repeat(87));

    for (id, agent) in &config.agents {
        let indicator = if agent.enabled {
            green.apply_to("\u{25cf}").to_string()
        } else {
            dim.apply_to("\u{25cb}").to_string()
        };

        let mcp = agent
            .mcp_config
            .as_deref()
            .unwrap_or("-");

        println!(
            "{} {:<12} {:<15} {:<30} {:<30}",
            indicator, id, agent.name, agent.skills_path, mcp
        );
    }
}

fn add_custom_agent(config: &mut Config) -> Result<()> {
    let short_name = inquire::Text::new("Short name (identifier):").prompt()?;
    let display_name = inquire::Text::new("Display name:").prompt()?;
    let skills_path = inquire::Text::new("Skills path:").prompt()?;
    let mcp_config_input = inquire::Text::new("MCP config path (empty to skip):")
        .with_default("")
        .prompt()?;
    let mcp_config = if mcp_config_input.is_empty() {
        None
    } else {
        Some(mcp_config_input)
    };

    let mcp_format = if mcp_config.is_some() {
        let fmt_choices = vec!["json", "toml"];
        let fmt = Select::new("MCP format:", fmt_choices).prompt()?;
        match fmt {
            "toml" => McpFormat::Toml,
            _ => McpFormat::Json,
        }
    } else {
        McpFormat::Json
    };

    let mcp_key = if mcp_config.is_some() {
        let key = inquire::Text::new("MCP key:")
            .with_default("mcpServers")
            .prompt()?;
        Some(key)
    } else {
        None
    };

    let agent = AgentConfig {
        name: display_name,
        skills_path,
        mcp_config,
        mcp_format,
        mcp_key,
        enabled: true,
    };

    config.agents.insert(short_name, agent);
    config.save()?;
    println!("Agent added and config saved.");

    Ok(())
}

fn remove_custom_agent(config: &mut Config) -> Result<()> {
    let custom_agents: Vec<String> = config
        .agents
        .keys()
        .filter(|k| !BUILTIN_AGENTS.contains(&k.as_str()))
        .cloned()
        .collect();

    if custom_agents.is_empty() {
        println!("No custom agents to remove.");
        return Ok(());
    }

    let selection = Select::new("Select agent to remove:", custom_agents).prompt()?;
    config.agents.remove(&selection);
    config.save()?;
    println!("Agent '{}' removed.", selection);

    Ok(())
}
