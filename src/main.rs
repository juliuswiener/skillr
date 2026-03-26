mod agents;
mod config;
mod lockfile;
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

fn main() {
    let cli = Cli::parse();

    match cli.command {
        None => {
            println!("(wizard not yet implemented)");
        }
        Some(Commands::Sync) => {
            println!("(not yet implemented)");
        }
        Some(Commands::Skills { action }) => match action {
            SkillsAction::List => println!("(not yet implemented)"),
            SkillsAction::Install { .. } => println!("(not yet implemented)"),
            SkillsAction::Remove { .. } => println!("(not yet implemented)"),
            SkillsAction::Sync => println!("(not yet implemented)"),
        },
        Some(Commands::Mcps { action }) => match action {
            McpsAction::List => println!("(not yet implemented)"),
            McpsAction::Add { .. } => println!("(not yet implemented)"),
            McpsAction::Remove { .. } => println!("(not yet implemented)"),
            McpsAction::Sync => println!("(not yet implemented)"),
            McpsAction::Import => println!("(not yet implemented)"),
        },
        Some(Commands::Market { action }) => match action {
            MarketAction::Browse { .. } => println!("(not yet implemented)"),
            MarketAction::Add { .. } => println!("(not yet implemented)"),
            MarketAction::Update => println!("(not yet implemented)"),
            MarketAction::List => println!("(not yet implemented)"),
            MarketAction::Remove { .. } => println!("(not yet implemented)"),
        },
    }
}
