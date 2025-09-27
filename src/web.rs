use anyhow::{Context, Result};
use std::process::Command;

use crate::utils;

pub fn open(query: &str, provider: Option<&str>) -> Result<()> {
    // 空のクエリの場合はヘルプを表示
    if query.trim().is_empty() {
        utils::print_warn("No query or URL provided");
        println!();
        println!("  Usage examples:");
        println!("    uwu web rust programming              # Search on Google");
        println!("    uwu web github.com                    # Open website");
        println!("    uwu web https://example.com           # Open URL");
        println!("    uwu web \"rust async\" -p youtube       # Search on YouTube");
        println!("    uwu web \"MacBook Pro\" -p amazon        # Search on Amazon");
        println!();
        println!("  Available providers:");
        println!("    google (default)    Google Search");
        println!("    bing               Bing Search");
        println!("    ddg                DuckDuckGo");
        println!("    youtube, yt        YouTube");
        println!("    github, gh         GitHub");
        println!("    stackoverflow, so  Stack Overflow");
        println!("    amazon             Amazon");
        println!("    twitter, x         Twitter/X");
        println!("    reddit             Reddit");
        return Ok(());
    }

    let url = if query.starts_with("http://") || query.starts_with("https://") {
        // すでにURLの場合はそのまま開く
        query.to_string()
    } else if query.contains('.') && !query.contains(' ') && !provider.is_some() {
        // ドメイン名っぽい場合（スペースがない＆ドットがある＆プロバイダー指定なし）
        if query.starts_with("www.") {
            format!("https://{}", query)
        } else {
            format!("https://{}", query)
        }
    } else {
        // 検索クエリとして扱う
        build_search_url(query, provider)?
    };

    if !utils::is_quiet() {
        let provider_name = provider.unwrap_or("direct");
        utils::print_info(&format!("Opening with {}: {}", provider_name,
                                   if url.len() > 60 {
                                       format!("{}...", &url[..60])
                                   } else {
                                       url.clone()
                                   }
        ));
    }

    // Windowsの既定のブラウザで開く
    #[cfg(windows)]
    {
        Command::new("cmd")
            .args(&["/C", "start", "", &url])
            .spawn()
            .context("Failed to open browser")?;
    }

    #[cfg(not(windows))]
    {
        // Linux/Mac用
        Command::new("xdg-open")
            .arg(&url)
            .spawn()
            .context("Failed to open browser")?;
    }

    utils::print_success("Browser opened");
    Ok(())
}

fn build_search_url(query: &str, provider: Option<&str>) -> Result<String> {
    // URLエンコード
    let encoded_query = urlencoding::encode(query);

    let url = match provider {
        Some("bing") => {
            format!("https://www.bing.com/search?q={}", encoded_query)
        },
        Some("ddg") | Some("duckduckgo") => {
            format!("https://duckduckgo.com/?q={}", encoded_query)
        },
        Some("youtube") | Some("yt") => {
            format!("https://www.youtube.com/results?search_query={}", encoded_query)
        },
        Some("github") | Some("gh") => {
            format!("https://github.com/search?q={}", encoded_query)
        },
        Some("stackoverflow") | Some("so") => {
            format!("https://stackoverflow.com/search?q={}", encoded_query)
        },
        Some("amazon") => {
            format!("https://www.amazon.com/s?k={}", encoded_query)
        },
        Some("twitter") | Some("x") => {
            format!("https://twitter.com/search?q={}", encoded_query)
        },
        Some("reddit") => {
            format!("https://www.reddit.com/search/?q={}", encoded_query)
        },
        Some(unknown) => {
            utils::print_warn(&format!("Unknown provider: {}", unknown));
            utils::print_info("Falling back to Google search");
            format!("https://www.google.com/search?q={}", encoded_query)
        },
        None => {
            // デフォルトはGoogle
            format!("https://www.google.com/search?q={}", encoded_query)
        }
    };

    Ok(url)
}