mod agents;
mod config;
mod lockfile;
mod market;
mod mcps;
mod skills;
mod util;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "skillr", about = "Unified AI agent skill & MCP manager")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Sync skills and MCP configs across all enabled agents
    Sync,

    /// Manage skills
    Skills {
        #[command(subcommand)]
        action: SkillsAction,
    },

    /// Manage MCP server configurations
    Mcps {
        #[command(subcommand)]
        action: McpsAction,
    },

    /// Browse and manage skill marketplaces
    Market {
        #[command(subcommand)]
        action: MarketAction,
    },
}

#[derive(Subcommand)]
enum SkillsAction {
    /// List all installed skills
    List,
    /// Install a skill from a local path, git repo, or marketplace
    Install {
        /// Source path, URL, or marketplace identifier
        source: Option<String>,
    },
    /// Remove an installed skill
    Remove {
        /// Name of the skill to remove
        name: Option<String>,
    },
    /// Sync skills to all enabled agents
    Sync,
}

#[derive(Subcommand)]
enum McpsAction {
    /// List all registered MCP servers
    List,
    /// Add an MCP server configuration
    Add {
        /// Name of the MCP server
        name: Option<String>,
    },
    /// Remove an MCP server configuration
    Remove {
        /// Name of the MCP server to remove
        name: Option<String>,
    },
    /// Sync MCP configs to all enabled agents
    Sync,
    /// Import MCP servers from an agent's existing config
    Import,
}

#[derive(Subcommand)]
enum MarketAction {
    /// Browse marketplace skills
    Browse {
        /// Search query
        query: Option<String>,
    },
    /// Add a marketplace repository
    Add {
        /// Git repository URL
        repo: String,
    },
    /// Update all marketplace indexes
    Update,
    /// List configured marketplaces
    List,
    /// Remove a marketplace
    Remove {
        /// Name of the marketplace to remove
        name: Option<String>,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        None => {
            println!("(wizard not yet implemented)");
        }
        Some(Commands::Sync) => {
            let config = config::Config::load()?;
            skills::sync::sync_skills(&config)?;
            mcps::sync::sync_mcps(&config)?;
        }
        Some(Commands::Skills { action }) => {
            let config = config::Config::load()?;
            match action {
                SkillsAction::List => skills::list::print_skills_table(&config)?,
                SkillsAction::Install { source } => {
                    skills::install::install_skill(&config, source.as_deref())?;
                }
                SkillsAction::Remove { name } => {
                    skills::remove::remove_skill(&config, name.as_deref())?;
                }
                SkillsAction::Sync => skills::sync::sync_skills(&config)?,
            }
        }
        Some(Commands::Mcps { action }) => {
            let config = config::Config::load()?;
            match action {
                McpsAction::List => mcps::list::list_mcps(&config)?,
                McpsAction::Add { name } => mcps::add::add_mcp(&config, name.as_deref())?,
                McpsAction::Remove { name } => mcps::remove::remove_mcp(&config, name.as_deref())?,
                McpsAction::Sync => mcps::sync::sync_mcps(&config)?,
                McpsAction::Import => mcps::sync::sync_mcps(&config)?,
            }
        }
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
        }
    }

    Ok(())
}
