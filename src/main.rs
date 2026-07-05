//! uwu - Windows Utilities
//!
//! A minimal and cute Windows utility toolkit written in Rust.

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};

mod alias;
mod color;
mod del;
mod env;
mod go;
mod init;
mod kill;
mod notify;
mod path;
mod shot;
mod tree;
mod utils;
mod wdex;
mod web;

#[derive(Parser)]
#[command(
    name = "uwu",
    version,
    about = "uwu ~ uwu's Windows Utilities",
    long_about = "uwu ~ uwu's Windows Utilities — a minimal and cute toolkit written in Rust"
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

    /// Manage command aliases
    Alias {
        /// Alias name (omit to list all)
        #[arg(value_name = "NAME")]
        name: Option<String>,

        /// Command to alias to (omit to show alias)
        #[arg(value_name = "COMMAND", trailing_var_arg = true)]
        command: Vec<String>,

        /// List all aliases
        #[arg(short, long)]
        ls: bool,

        /// Remove the alias
        #[arg(short, long)]
        rm: bool,

        /// Output loader script for PowerShell profile
        #[arg(long, hide = true)]
        load: bool,
    },

    /// Manage environment variables
    Env {
        /// Variable name (omit to list all)
        #[arg(value_name = "KEY")]
        key: Option<String>,

        /// Value to set (omit to show)
        #[arg(value_name = "VALUE")]
        value: Option<String>,

        /// Target the system (machine) scope — requires admin
        #[arg(short, long)]
        system: bool,

        /// Remove (unset) the variable
        #[arg(short, long)]
        rm: bool,
    },

    /// Jump to or manage directory bookmarks
    Go {
        /// Bookmark name (omit to list all)
        #[arg(value_name = "ALIAS")]
        alias: Option<String>,

        /// Directory to bookmark (omit to jump/show)
        #[arg(value_name = "PATH")]
        path: Option<String>,

        /// Remove the bookmark
        #[arg(short, long)]
        rm: bool,

        /// Print the resolved path only (used by the shell wrapper)
        #[arg(long, hide = true)]
        resolve: bool,
    },

    /// Kill processes by name, port, or PID (safe)
    Kill {
        /// Target: a port number, or a process name (glob: *, ?)
        #[arg(value_name = "TARGET")]
        target: Option<String>,

        /// Kill the process on this TCP port
        #[arg(short, long)]
        port: Option<u16>,

        /// Kill by process ID
        #[arg(long)]
        pid: Option<u32>,

        /// Kill by process name (glob)
        #[arg(short = 'N', long)]
        name: Option<String>,

        /// Kill without confirmation
        #[arg(short, long)]
        force: bool,

        /// Dry run - show what would be killed
        #[arg(short = 'n', long)]
        dry_run: bool,
    },

    /// Print a directory tree (respects .gitignore)
    Tree {
        /// Root directory (default: current directory)
        #[arg(value_name = "PATH")]
        path: Option<String>,

        /// Show everything: hidden files and .gitignored entries
        #[arg(short, long)]
        all: bool,

        /// Use ASCII connectors instead of Unicode box-drawing
        #[arg(long)]
        ascii: bool,

        /// Maximum depth to descend
        #[arg(short = 'L', long)]
        level: Option<usize>,

        /// List directories only
        #[arg(short, long)]
        dirs: bool,
    },

    /// Capture a screenshot to a file or the clipboard
    Shot {
        /// Output file (PNG/JPG/BMP). Omit to auto-name uwu_screenshot_<timestamp>.png
        #[arg(value_name = "FILE")]
        file: Option<String>,

        /// Copy to the clipboard instead of saving a file
        #[arg(short = 'c', long)]
        clip: bool,

        /// Capture the active window instead of the full screen
        #[arg(short, long)]
        window: bool,

        /// Interactive region selection (Win+Shift+S style)
        #[arg(short, long)]
        region: bool,

        /// Delay in seconds before capturing
        #[arg(short, long, default_value_t = 0)]
        delay: u64,
    },

    /// Send a Windows toast notification
    Notify {
        /// Message body to show (multiple words allowed)
        #[arg(value_name = "MESSAGE")]
        message: Vec<String>,

        /// Notification title
        #[arg(short, long, default_value = "uwu")]
        title: String,
    },

    /// Initialize uwu (set up PowerShell profile)
    Init,

    /// Reload shell session (apply alias changes)
    Reload,

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

    // Handle alias --load silently (used by PowerShell profile, no banner/footer)
    if let Command::Alias { load: true, .. } = &cli.command {
        return alias::load();
    }

    // Handle `go --resolve <name>` silently (used by the PowerShell wrapper for cd)
    if let Command::Go {
        resolve: true,
        alias,
        ..
    } = &cli.command
    {
        return match alias {
            Some(name) => go::resolve(name),
            None => anyhow::bail!("go --resolve requires a bookmark name"),
        };
    }

    // Configure global settings
    configure_environment(&cli);

    // Route to appropriate command handler
    let result = match cli.command {
        Command::Alias {
            name,
            command,
            ls,
            rm,
            ..
        } => handle_alias_command(name, command, ls, rm),
        Command::Env {
            key,
            value,
            system,
            rm,
        } => handle_env_command(key, value, system, rm),
        Command::Go {
            alias,
            path,
            rm,
            ..
        } => handle_go_command(alias, path, rm),
        Command::Kill {
            target,
            port,
            pid,
            name,
            force,
            dry_run,
        } => handle_kill_command(target, port, pid, name, force, dry_run),
        Command::Tree {
            path,
            all,
            ascii,
            level,
            dirs,
        } => handle_tree_command(path, all, ascii, level, dirs),
        Command::Shot {
            file,
            clip,
            window,
            region,
            delay,
        } => handle_shot_command(file, clip, window, region, delay),
        Command::Notify { message, title } => handle_notify_command(message, title),
        Command::Init => init::run(),
        Command::Reload => {
            println!();
            utils::print_warn("reload is handled by the PowerShell wrapper.");
            println!("  run '{}' first to set it up.", color::accent("uwu init"));
            Ok(())
        }
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

    result
}

/// Configure the application environment based on CLI flags
fn configure_environment(cli: &Cli) {
    utils::set_quiet_mode(cli.quiet);

    // Initialize crossterm colors
    color::init_colors();
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

/// Handle environment variable commands
fn handle_env_command(
    key: Option<String>,
    value: Option<String>,
    system: bool,
    rm: bool,
) -> Result<()> {
    match (key, value, rm) {
        (None, _, _) => env::list(system),
        (Some(k), _, true) => env::remove(&k, system),
        (Some(k), Some(v), false) => env::set(&k, &v, system),
        (Some(k), None, false) => env::get(&k, system),
    }
}

/// Handle directory bookmark commands
fn handle_go_command(alias: Option<String>, path: Option<String>, rm: bool) -> Result<()> {
    match (alias, path, rm) {
        (None, _, _) => go::list(),
        (Some(a), _, true) => go::remove(&a),
        (Some(a), Some(p), false) => go::set(&a, &p),
        (Some(a), None, false) => go::show(&a),
    }
}

/// Handle process kill commands
fn handle_kill_command(
    target: Option<String>,
    port: Option<u16>,
    pid: Option<u32>,
    name: Option<String>,
    force: bool,
    dry_run: bool,
) -> Result<()> {
    println!();
    let selector = kill::resolve_selector(target, port, pid, name)?;
    kill::run(selector, force, dry_run)
}

/// Handle directory tree commands
fn handle_tree_command(
    path: Option<String>,
    all: bool,
    ascii: bool,
    level: Option<usize>,
    dirs: bool,
) -> Result<()> {
    println!();
    tree::print(path.as_deref().unwrap_or("."), all, ascii, level, dirs)
}

/// Handle screenshot commands
fn handle_shot_command(
    file: Option<String>,
    clip: bool,
    window: bool,
    region: bool,
    delay: u64,
) -> Result<()> {
    println!();
    if region {
        println!("  {} region...", color::info("select a"));
    } else {
        println!("  {} screenshot...", color::info("capturing"));
    }

    match shot::capture(file.as_deref(), window, region, clip, delay)? {
        Some(path) => utils::print_success(&format!("saved: {}", path)),
        None => utils::print_success("copied to clipboard"),
    }
    Ok(())
}

/// Handle notify commands
fn handle_notify_command(message: Vec<String>, title: String) -> Result<()> {
    let body = message.join(" ");
    if body.trim().is_empty() {
        bail!("notify: a message is required");
    }

    println!();
    println!("  {} notification...", color::info("sending"));
    notify::send(&title, &body)?;
    utils::print_success("notification sent");
    Ok(())
}

/// Handle alias commands
fn handle_alias_command(
    name: Option<String>,
    command: Vec<String>,
    ls: bool,
    rm: bool,
) -> Result<()> {
    if ls {
        return alias::list();
    }

    match name {
        None => alias::list(),
        Some(name) if rm => alias::remove(&name),
        Some(name) if command.is_empty() => alias::show(&name),
        Some(name) => {
            let cmd = command.join(" ");
            alias::add(&name, &cmd)
        }
    }
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
