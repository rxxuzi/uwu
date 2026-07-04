//! Directory tree module for uwu.
//!
//! Prints a `tree`-style listing that respects `.gitignore` by default (using
//! the same `ignore` engine as ripgrep/fd). Supports Unicode or ASCII branch
//! connectors, an "everything" mode (`--all`), depth limiting, and dirs-only.

use anyhow::{bail, Result};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

use crate::color;

/// Branch connector glyphs.
struct Glyphs {
    tee: &'static str,
    last: &'static str,
    vert: &'static str,
    space: &'static str,
}

const UNICODE: Glyphs = Glyphs {
    tee: "├── ",
    last: "└── ",
    vert: "│   ",
    space: "    ",
};

const ASCII: Glyphs = Glyphs {
    tee: "|-- ",
    last: "`-- ",
    vert: "|   ",
    space: "    ",
};

/// A single collected entry.
struct Entry {
    path: PathBuf,
    is_dir: bool,
}

/// Print a directory tree rooted at `root`.
///
/// * `all` - show hidden files and ignore `.gitignore` (show everything)
/// * `ascii` - use ASCII connectors instead of Unicode box-drawing
/// * `level` - maximum depth to descend
/// * `dirs_only` - list directories only
pub fn print(
    root: &str,
    all: bool,
    ascii: bool,
    level: Option<usize>,
    dirs_only: bool,
) -> Result<()> {
    let root_path = PathBuf::from(root);
    if !root_path.exists() {
        bail!("path not found: {}", root);
    }

    let entries = collect(&root_path, all, level, dirs_only);

    // Group children by their parent directory for tree rendering.
    let mut children: HashMap<PathBuf, Vec<Entry>> = HashMap::new();
    for e in entries {
        let parent = e.path.parent().unwrap_or(Path::new("")).to_path_buf();
        children.entry(parent).or_default().push(e);
    }

    let glyphs = if ascii { &ASCII } else { &UNICODE };

    // Root line, then the tree.
    println!("  {}", color::accent(root));
    let mut counts = Counts::default();
    render(&root_path, "  ".to_string(), &children, glyphs, &mut counts);

    println!();
    println!(
        "  {}, {}",
        color::note(&pluralize(counts.dirs, "directory", "directories")),
        color::note(&pluralize(counts.files, "file", "files")),
    );

    Ok(())
}

#[derive(Default)]
struct Counts {
    dirs: usize,
    files: usize,
}

/// Walk `root` with the ignore engine and return every entry below it (not the
/// root itself), sorted by name within each directory.
///
/// By default (not `--all`, not dirs-only) directories with no visible files
/// anywhere below them are pruned. This is what makes a whitelist-style
/// `.gitignore` read correctly: `!*/` re-includes every directory, so `target/`,
/// `archive/`, etc. would otherwise show as empty boxes even though all their
/// contents are ignored.
fn collect(root: &Path, all: bool, level: Option<usize>, dirs_only: bool) -> Vec<Entry> {
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(!all) // skip dotfiles unless --all
        .git_ignore(!all)
        .git_global(!all)
        .git_exclude(!all)
        .ignore(!all)
        .parents(!all)
        .require_git(false) // honor a stray .gitignore even outside a repo
        .follow_links(false)
        .max_depth(level);
    builder.sort_by_file_name(|a, b| a.cmp(b));
    // Always skip `.git` (enormous, never useful in a tree). Also treat
    // dot-prefixed names as hidden unless --all: the `ignore` crate keys "hidden"
    // off the Windows hidden *attribute*, so Unix-style dotfiles would otherwise leak.
    let skip_hidden = !all;
    builder.filter_entry(move |e| {
        if e.depth() == 0 {
            return true; // never filter the root itself
        }
        let name = e.file_name().to_string_lossy();
        if name == ".git" {
            return false;
        }
        !(skip_hidden && name.starts_with('.'))
    });

    let mut out = Vec::new();
    for result in builder.build() {
        let entry = match result {
            Ok(e) => e,
            Err(_) => continue,
        };
        if entry.depth() == 0 {
            continue; // the root itself
        }
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if dirs_only && !is_dir {
            continue;
        }
        out.push(Entry {
            path: entry.path().to_path_buf(),
            is_dir,
        });
    }

    // Prune directories with no visible files below them.
    if !all && !dirs_only {
        let mut keep: HashSet<PathBuf> = HashSet::new();
        for e in &out {
            if e.is_dir {
                continue;
            }
            // Mark every ancestor directory of this file (up to root) as kept.
            let mut cur = e.path.parent();
            while let Some(dir) = cur {
                if !keep.insert(dir.to_path_buf()) || dir == root {
                    break;
                }
                cur = dir.parent();
            }
        }
        out.retain(|e| !e.is_dir || keep.contains(&e.path));
    }

    out
}

/// Recursively render the children of `dir`.
fn render(
    dir: &Path,
    prefix: String,
    children: &HashMap<PathBuf, Vec<Entry>>,
    glyphs: &Glyphs,
    counts: &mut Counts,
) {
    let kids = match children.get(dir) {
        Some(k) => k,
        None => return,
    };

    let n = kids.len();
    for (i, entry) in kids.iter().enumerate() {
        let last = i == n - 1;
        let branch = if last { glyphs.last } else { glyphs.tee };
        let name = entry
            .path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        if entry.is_dir {
            counts.dirs += 1;
            println!("{}{}{}", prefix, branch, color::accent(&name));
            let extension = if last { glyphs.space } else { glyphs.vert };
            render(
                &entry.path,
                format!("{}{}", prefix, extension),
                children,
                glyphs,
                counts,
            );
        } else {
            counts.files += 1;
            println!("{}{}{}", prefix, branch, color::info(&name));
        }
    }
}

/// "1 file" / "3 files"
fn pluralize(n: usize, singular: &str, plural: &str) -> String {
    format!("{} {}", n, if n == 1 { singular } else { plural })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn pluralize_singular_and_plural() {
        assert_eq!(pluralize(1, "file", "files"), "1 file");
        assert_eq!(pluralize(0, "file", "files"), "0 files");
        assert_eq!(pluralize(3, "directory", "directories"), "3 directories");
    }

    #[test]
    fn glyphs_differ_between_modes() {
        assert!(UNICODE.tee.starts_with('├'));
        assert!(ASCII.tee.starts_with('|'));
    }

    fn names(entries: &[Entry]) -> Vec<String> {
        entries
            .iter()
            .map(|e| e.path.file_name().unwrap().to_string_lossy().to_string())
            .collect()
    }

    #[test]
    fn respects_gitignore_by_default() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join(".gitignore"), "ignored.txt\nbuild/\n").unwrap();
        fs::write(root.join("keep.txt"), "").unwrap();
        fs::write(root.join("ignored.txt"), "").unwrap();
        fs::create_dir(root.join("build")).unwrap();
        fs::write(root.join("build").join("out.o"), "").unwrap();

        let got = names(&collect(root, false, None, false));
        assert!(got.contains(&"keep.txt".to_string()));
        assert!(!got.contains(&"ignored.txt".to_string()));
        assert!(!got.contains(&"build".to_string()));
        // .gitignore is a dotfile → hidden by default
        assert!(!got.contains(&".gitignore".to_string()));
    }

    #[test]
    fn all_flag_shows_everything() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join(".gitignore"), "ignored.txt\n").unwrap();
        fs::write(root.join("keep.txt"), "").unwrap();
        fs::write(root.join("ignored.txt"), "").unwrap();

        let got = names(&collect(root, true, None, false));
        assert!(got.contains(&"keep.txt".to_string()));
        assert!(got.contains(&"ignored.txt".to_string()));
        assert!(got.contains(&".gitignore".to_string()));
    }

    #[test]
    fn always_skips_dot_git() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::create_dir(root.join(".git")).unwrap();
        fs::write(root.join(".git").join("config"), "").unwrap();
        fs::write(root.join("keep.txt"), "").unwrap();

        // Even with --all, .git must not appear.
        let got = names(&collect(root, true, None, false));
        assert!(!got.contains(&".git".to_string()));
        assert!(!got.contains(&"config".to_string()));
    }

    #[test]
    fn prunes_empty_dirs_by_default() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        // A directory whose only content is git-ignored -> effectively empty.
        fs::write(root.join(".gitignore"), "junk/*\n").unwrap();
        fs::create_dir(root.join("junk")).unwrap();
        fs::write(root.join("junk").join("a.tmp"), "").unwrap();
        // A real file so the tree isn't entirely empty.
        fs::write(root.join("keep.txt"), "").unwrap();

        let default = names(&collect(root, false, None, false));
        assert!(default.contains(&"keep.txt".to_string()));
        // junk/ has no visible files -> pruned.
        assert!(!default.contains(&"junk".to_string()));

        // With --all, the ignored dir and its file reappear (no pruning).
        let all = names(&collect(root, true, None, false));
        assert!(all.contains(&"junk".to_string()));
        assert!(all.contains(&"a.tmp".to_string()));
    }

    #[test]
    fn dirs_only_excludes_files() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::create_dir(root.join("sub")).unwrap();
        fs::write(root.join("sub").join("a.txt"), "").unwrap();
        fs::write(root.join("top.txt"), "").unwrap();

        let got = names(&collect(root, true, None, true));
        assert!(got.contains(&"sub".to_string()));
        assert!(!got.contains(&"a.txt".to_string()));
        assert!(!got.contains(&"top.txt".to_string()));
    }

    #[test]
    fn level_limits_depth() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("a").join("b")).unwrap();
        fs::write(root.join("a").join("b").join("deep.txt"), "").unwrap();

        let got = names(&collect(root, true, Some(1), false));
        assert!(got.contains(&"a".to_string()));
        // depth 2+ excluded
        assert!(!got.contains(&"b".to_string()));
        assert!(!got.contains(&"deep.txt".to_string()));
    }
}
