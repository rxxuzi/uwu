use anyhow::Result;
use clap::{Parser, Subcommand};

mod path;
mod wdex;
mod web;
mod utils;

#[derive(Parser)]
#[command(
name = "uwu",
version,
about = "UWU - Windows Utilities",
long_about = "A minimal Windows utility toolkit written in Rust"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Suppress informational output and disable colors
    #[arg(short, long, global = true)]
    quiet: bool,
}

#[derive(Subcommand)]
enum Command {
    /// Manage PATH environment variable
    Path {
        #[command(subcommand)]
        action: PathAction,
    },

    /// Manage Windows Defender exclusions
    Wdex {
        #[command(subcommand)]
        action: WdexAction,
    },

    /// Open URL or search in browser
    Web {
        /// Search query or URL to open
        query: Vec<String>,

        /// Use specific search provider (google, bing, ddg, youtube, github, stackoverflow, amazon, twitter, reddit)
        #[arg(short = 'p', long)]
        provider: Option<String>,
    },
}

#[derive(Subcommand)]
enum PathAction {
    /// Add a directory to PATH
    Add {
        /// Path to add
        path: String,

        /// Target system PATH (requires admin)
        #[arg(short, long)]
        system: bool,

        /// Force add without confirmation
        #[arg(short, long)]
        force: bool,
    },

    /// Remove a directory from PATH
    Remove {
        /// Path to remove
        path: String,

        /// Target system PATH (requires admin)
        #[arg(short, long)]
        system: bool,

        /// Force remove without confirmation
        #[arg(short, long)]
        force: bool,
    },

    /// List all PATH entries
    List {
        /// Show system PATH
        #[arg(short, long)]
        system: bool,
    },

    /// Clean invalid PATH entries
    Clean {
        /// Target system PATH (requires admin)
        #[arg(short, long)]
        system: bool,

        /// Force clean without confirmation
        #[arg(short, long)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum WdexAction {
    /// Add path to Windows Defender exclusions
    Add {
        /// Path to exclude
        path: String,

        /// Add as process exclusion
        #[arg(short, long)]
        process: bool,
    },

    /// Remove path from Windows Defender exclusions
    Remove {
        /// Path to remove
        path: String,

        /// Remove from process exclusions
        #[arg(short, long)]
        process: bool,
    },

    /// List all Windows Defender exclusions
    List,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Set quiet mode and color preference
    utils::set_quiet_mode(cli.quiet);
    if cli.quiet {
        colored::control::set_override(false);
    }

    // Windows-specific: Enable ANSI colors
    #[cfg(windows)]
    {
        let _ = colored::control::set_virtual_terminal(true);
    }

    match cli.command {
        Command::Path { action } => {
            handle_path_command(action)?;
        }
        Command::Wdex { action } => {
            handle_wdex_command(action)?;
        }
        Command::Web { query, provider } => {
            let search_query = query.join(" ");
            web::open(&search_query, provider.as_deref())?;
        }
    }

    Ok(())
}

fn handle_path_command(action: PathAction) -> Result<()> {
    match action {
        PathAction::Add { path, system, force } => {
            path::add(&path, system, force)?;
        }
        PathAction::Remove { path, system, force } => {
            path::remove(&path, system, force)?;
        }
        PathAction::List { system } => {
            path::list(system)?;
        }
        PathAction::Clean { system, force } => {
            path::clean(system, force)?;
        }
    }
    Ok(())
}

fn handle_wdex_command(action: WdexAction) -> Result<()> {
    match action {
        WdexAction::Add { path, process } => {
            wdex::add(&path, process)?;
        }
        WdexAction::Remove { path, process } => {
            wdex::remove(&path, process)?;
        }
        WdexAction::List => {
            wdex::list()?;
        }
    }
    Ok(())
}