use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::io::{self, Write};

use crate::utils;

pub fn add(path_str: &str, process: bool) -> Result<()> {
    // 管理者権限チェック
    if !utils::is_elevated() {
        return run_elevated("add", path_str, process);
    }

    let path = if !process {
        // パスの場合は絶対パスに変換
        let p = PathBuf::from(path_str);
        if p.is_absolute() {
            p
        } else {
            std::env::current_dir()?.join(&p)
        }
    } else {
        // プロセスの場合はそのまま
        PathBuf::from(path_str)
    };

    let path_string = path.to_string_lossy().to_string();

    // パスが存在するかチェック（プロセスでない場合）
    if !process && !path.exists() {
        utils::print_warn(&format!("Path does not exist: {}", path_string));

        print!("  Add anyway? [y/N]: ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            utils::print_info("Cancelled.");
            return Ok(());
        }
    }

    // PowerShellコマンドを構築
    let ps_command = if process {
        format!("Add-MpPreference -ExclusionProcess '{}'", path_string)
    } else {
        format!("Add-MpPreference -ExclusionPath '{}'", path_string)
    };

    // PowerShellで実行
    let output = Command::new("powershell")
        .args(&[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &ps_command
        ])
        .output()
        .context("Failed to execute PowerShell")?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to add exclusion: {}", error);
    }

    utils::print_success(&format!(
        "Added to Windows Defender exclusions: {}",
        path_string
    ));

    Ok(())
}

pub fn remove(path_str: &str, process: bool) -> Result<()> {
    // 管理者権限チェック
    if !utils::is_elevated() {
        return run_elevated("remove", path_str, process);
    }

    let path = if !process {
        let p = PathBuf::from(path_str);
        if p.is_absolute() {
            p
        } else {
            std::env::current_dir()?.join(&p)
        }
    } else {
        PathBuf::from(path_str)
    };

    let path_string = path.to_string_lossy().to_string();

    // 確認
    print!("  Remove from exclusions? [y/N]: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    if !input.trim().eq_ignore_ascii_case("y") {
        utils::print_info("Cancelled.");
        return Ok(());
    }

    // PowerShellコマンドを構築
    let ps_command = if process {
        format!("Remove-MpPreference -ExclusionProcess '{}'", path_string)
    } else {
        format!("Remove-MpPreference -ExclusionPath '{}'", path_string)
    };

    // PowerShellで実行
    let output = Command::new("powershell")
        .args(&[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &ps_command
        ])
        .output()
        .context("Failed to execute PowerShell")?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to remove exclusion: {}", error);
    }

    utils::print_success(&format!(
        "Removed from Windows Defender exclusions: {}",
        path_string
    ));

    Ok(())
}

pub fn list() -> Result<()> {
    // PowerShellで除外リストを取得（シンプルなテキスト形式）
    let ps_commands = vec![
        ("Path exclusions", "(Get-MpPreference).ExclusionPath"),
        ("Process exclusions", "(Get-MpPreference).ExclusionProcess"),
        ("Extension exclusions", "(Get-MpPreference).ExclusionExtension"),
    ];

    println!();

    for (title, command) in ps_commands {
        let output = Command::new("powershell")
            .args(&[
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                command
            ])
            .output()
            .context("Failed to execute PowerShell")?;

        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            let items: Vec<&str> = output_str
                .lines()
                .filter(|line| !line.trim().is_empty())
                .collect();

            if !items.is_empty() {
                println!("  {}:", utils::cyan(title));
                for item in items {
                    // パス除外の場合、存在チェック
                    if title.contains("Path") {
                        let exists = Path::new(item.trim()).exists();
                        if exists {
                            println!("    - {}", item.trim());
                        } else {
                            println!("    - {} {}",
                                     item.trim(),
                                     utils::pink("(not found)")
                            );
                        }
                    } else {
                        println!("    - {}", item.trim());
                    }
                }
                println!();
            }
        }
    }

    // すべて空の場合のメッセージ
    let all_empty_check = Command::new("powershell")
        .args(&[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$p = Get-MpPreference; if($p.ExclusionPath -or $p.ExclusionProcess -or $p.ExclusionExtension){'has'}else{'empty'}"
        ])
        .output()?;

    let check_result = String::from_utf8_lossy(&all_empty_check.stdout);
    if check_result.trim() == "empty" {
        println!("  {}", utils::lavender("No exclusions configured."));
        println!();
    }

    if !utils::is_elevated() {
        println!("  {}", utils::lavender("Note: Some details may require administrator privileges."));
    }

    Ok(())
}

// 管理者権限で再実行
fn run_elevated(action: &str, path: &str, process: bool) -> Result<()> {
    utils::print_warn("Administrator privileges required.");

    print!("  Restart as administrator? [y/N]: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    if !input.trim().eq_ignore_ascii_case("y") {
        utils::print_info("Cancelled.");
        return Ok(());
    }

    let exe_path = std::env::current_exe()?;
    let mut args = vec!["wdex", action, path];
    if process {
        args.push("--process");
    }

    // Windows用のShellExecute（昇格）
    #[cfg(windows)]
    {
        use windows::Win32::UI::Shell::*;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        use windows::core::*;

        let exe_path_str = exe_path.to_string_lossy();
        let args_str = args.join(" ");

        unsafe {
            let result = ShellExecuteW(
                None,
                w!("runas"),
                PCWSTR::from_raw(exe_path_str.encode_utf16().chain(std::iter::once(0)).collect::<Vec<_>>().as_ptr()),
                PCWSTR::from_raw(args_str.encode_utf16().chain(std::iter::once(0)).collect::<Vec<_>>().as_ptr()),
                None,
                SW_SHOWNORMAL
            );

            if result.0 <= 32 {
                bail!("Failed to run as administrator");
            }
        }
    }

    utils::print_info("Command executed in elevated process.");
    Ok(())
}