//! uwu - Windows Utilities
//!
//! A minimal and cute Windows utility toolkit written in Rust.

use anyhow::Result;
use clap::{Parser, Subcommand};

mod color;
mod del;
mod path;
mod utils;
mod wdex;
mod web;

#[derive(Parser)]
#[command(
    name = "uwu",
    version,
    about = "uwu ~ windows utilities",
    long_about = "a minimal and cute windows utility toolkit written in rust ~"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Suppress informational output
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

        /// Use specific search provider
        #[arg(short = 'p', long)]
        provider: Option<String>,
    },

    /// Delete files/directories recursively (safe rm -rf)
    Del {
        /// Path to delete
        path: String,

        /// Force delete without confirmation
        #[arg(short, long)]
        force: bool,

        /// Dry run - show what would be deleted
        #[arg(short = 'n', long)]
        dry_run: bool,

        /// Verbose - show each file being deleted
        #[arg(short, long)]
        verbose: bool,
    },
}

#[derive(Subcommand)]
enum PathAction {
    /// Add a directory to PATH
    Add {
        /// Path to add (absolute or relative)
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
        /// Path to remove (index or path)
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

    /// Clean invalid and duplicate PATH entries
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

    // Configure global settings
    configure_environment(&cli);

    // Show a cute banner if not quiet
    if !cli.quiet {
        show_banner();
    }

    // Route to appropriate command handler
    let result = match cli.command {
        Command::Path { action } => handle_path_command(action),
        Command::Wdex { action } => handle_wdex_command(action),
        Command::Web { query, provider } => handle_web_command(query, provider),
        Command::Del {
            path,
            force,
            dry_run,
            verbose,
        } => handle_del_command(path, force, dry_run, verbose),
    };

    // Show a cute footer if successful and not quiet
    if result.is_ok() && !cli.quiet {
        show_footer();
    }

    result
}

/// Configure the application environment based on CLI flags
fn configure_environment(cli: &Cli) {
    utils::set_quiet_mode(cli.quiet);

    // Initialize crossterm colors
    color::init_colors();
}

/// Show a cute banner
fn show_banner() {
    println!();
    println!(
        "  {} ~ {}",
        color::accent("uwu"),
        color::info("windows utilities")
    );
}

/// Show a cute footer
fn show_footer() {
    println!();
    println!("  {}", color::note("~ done ~"));
    println!();
}

/// Handle PATH management commands
fn handle_path_command(action: PathAction) -> Result<()> {
    println!();
    match action {
        PathAction::Add {
            path,
            system,
            force,
        } => {
            println!("  {} path entry...", color::info("adding"));
            path::add(&path, system, force)
        }
        PathAction::Remove {
            path,
            system,
            force,
        } => {
            println!("  {} path entry...", color::info("removing"));
            path::remove(&path, system, force)
        }
        PathAction::List { system } => {
            println!("  {} path entries...", color::info("listing"));
            path::list(system)
        }
        PathAction::Clean { system, force } => {
            println!("  {} path entries...", color::info("cleaning"));
            path::clean(system, force)
        }
    }
}

/// Handle Windows Defender exclusion commands
fn handle_wdex_command(action: WdexAction) -> Result<()> {
    println!();
    match action {
        WdexAction::Add { path, process } => {
            println!("  {} defender exclusion...", color::info("adding"));
            wdex::add(&path, process)
        }
        WdexAction::Remove { path, process } => {
            println!("  {} defender exclusion...", color::info("removing"));
            wdex::remove(&path, process)
        }
        WdexAction::List => {
            println!("  {} defender exclusions...", color::info("listing"));
            wdex::list()
        }
    }
}

/// Handle web browser commands
fn handle_web_command(query: Vec<String>, provider: Option<String>) -> Result<()> {
    let search_query = query.join(" ");
    web::open(&search_query, provider.as_deref())
}

/// Handle delete commands
fn handle_del_command(path: String, force: bool, dry_run: bool, verbose: bool) -> Result<()> {
    println!();
    if dry_run {
        println!("  {} files...", color::info("previewing"));
    } else {
        println!("  {} files...", color::info("deleting"));
    }

    let summary = del::delete(&path, force, dry_run, verbose)?;

    if summary.files_deleted > 0 || summary.dirs_deleted > 0 {
        println!();
        println!(
            "  {} {}",
            color::success_symbol(),
            del::format_summary(&summary)
        );
    }

    Ok(())
}
