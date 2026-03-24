//! Web browser interaction module for uwu.
//!
//! Opens URLs and performs web searches with various providers.

use anyhow::{Context, Result};
use std::process::Command;

use crate::{color, utils};

/// Opens a URL or performs a web search in the default browser.
pub fn open(query: &str, provider: Option<&str>) -> Result<()> {
    // Show help if no query provided
    if query.trim().is_empty() {
        show_help();
        return Ok(());
    }

    let url = determine_url(query, provider)?;

    display_opening_message(&url, provider);

    open_in_browser(&url)?;

    utils::print_success("opened browser ~");
    Ok(())
}

/// Shows help information with colored output
fn show_help() {
    println!();
    println!("  {}", color::warn("no query provided..."));
    println!();

    println!("  {}", color::header("how to use:"));
    println!("    uwu web rust programming");
    println!("    uwu web github.com");
    println!("    uwu web https://example.com");
    println!("    uwu web \"rust async\" -p youtube");
    println!();

    println!("  {}", color::header("search providers:"));
    println!("    {}  google search", color::accent("google"));
    println!("    {}     bing search", color::accent("bing"));
    println!("    {}      duckduckgo", color::accent("ddg"));
    println!("    {}      youtube", color::accent("yt"));
    println!("    {}      github", color::accent("gh"));
    println!("    {}      stack overflow", color::accent("so"));
    println!("    {}   amazon", color::accent("amazon"));
    println!("    {}        twitter/x", color::accent("x"));
    println!("    {}   reddit", color::accent("reddit"));
}

/// Determines the final URL based on input
fn determine_url(query: &str, provider: Option<&str>) -> Result<String> {
    // Already a full URL
    if query.starts_with("http://") || query.starts_with("https://") {
        return Ok(query.to_string());
    }

    // Looks like a domain name
    if query.contains('.') && !query.contains(' ') && provider.is_none() {
        return Ok(format!("https://{}", query));
    }

    // Build search URL
    build_search_url(query, provider)
}

/// Displays the opening message
fn display_opening_message(url: &str, provider: Option<&str>) {
    if utils::is_quiet() {
        return;
    }

    let provider_name = match provider {
        Some("ddg") | Some("duckduckgo") => "duckduckgo",
        Some("yt") | Some("youtube") => "youtube",
        Some("gh") | Some("github") => "github",
        Some("so") | Some("stackoverflow") => "stack overflow",
        Some("x") | Some("twitter") => "twitter",
        Some(p) => p,
        None if url.starts_with("https://www.google.com") => "google",
        None => "browser",
    };

    let short_url = if url.len() > 60 {
        format!("{}...", &url[..60])
    } else {
        url.to_string()
    };

    println!();
    println!("  opening with {}...", color::accent(provider_name));
    println!("  {}", color::info(&short_url));
}

/// Builds a search URL for the given query and provider
fn build_search_url(query: &str, provider: Option<&str>) -> Result<String> {
    let encoded = urlencoding::encode(query);

    let url = match provider {
        Some("bing") => format!("https://www.bing.com/search?q={}", encoded),
        Some("ddg") | Some("duckduckgo") => format!("https://duckduckgo.com/?q={}", encoded),
        Some("youtube") | Some("yt") => {
            format!("https://www.youtube.com/results?search_query={}", encoded)
        }
        Some("github") | Some("gh") => format!("https://github.com/search?q={}", encoded),
        Some("stackoverflow") | Some("so") => {
            format!("https://stackoverflow.com/search?q={}", encoded)
        }
        Some("amazon") => format!("https://www.amazon.com/s?k={}", encoded),
        Some("twitter") | Some("x") => format!("https://twitter.com/search?q={}", encoded),
        Some("reddit") => format!("https://www.reddit.com/search/?q={}", encoded),
        Some(unknown) => {
            println!();
            println!(
                "  {} unknown provider: {}",
                color::warn("!"),
                color::warn(unknown)
            );
            println!("  using google instead...");
            format!("https://www.google.com/search?q={}", encoded)
        }
        None => format!("https://www.google.com/search?q={}", encoded),
    };

    Ok(url)
}

/// Opens the URL in the system's default browser
fn open_in_browser(url: &str) -> Result<()> {
    #[cfg(windows)]
    {
        Command::new("cmd")
            .args(&["/C", "start", "", url])
            .spawn()
            .context("Failed to open browser")?;
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(url)
            .spawn()
            .context("Failed to open browser")?;
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(url)
            .spawn()
            .context("Failed to open browser")?;
    }

    Ok(())
}
