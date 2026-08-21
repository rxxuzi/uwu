# uwu

> **uwu** ~ **u**wu's **W**indows **U**tilities — a minimal and cute toolkit written in Rust![img.png](img.png)

**uwu** bundles the little Windows chores that usually mean digging through
`reg.exe`, `netsh`, Task Manager, or a new terminal into one small, fast CLI.
Everything is native (no PowerShell round-trips), respects your `.gitignore`,
and treats destructive actions with care.

```
uwu path add ./bin      # add to PATH, then `uwu reload` — no new terminal
uwu kill 3000           # free the port stuck on "address already in use"
uwu tree                # a tree that honours .gitignore
uwu shot                # screenshot the screen to a PNG
uwu notify "build done" # toast notification when a long job finishes
```

## Install

Requires [Rust](https://rustup.rs/) and Windows.

```bash
git clone https://github.com/rxxuzi/uwu
cd uwu
cargo build --release
# binary at target/release/uwu.exe
```

Add it to your PATH, then wire up the shell integration:

```bash
uwu path add ./target/release   # put uwu.exe on PATH
uwu init                        # set up the PowerShell profile (uwu reload, aliases)
```

Open a new terminal once, and you're set.

## Commands

### `path` — PATH management
```bash
uwu path add ./bin       # add a directory (paths are normalized)
uwu path remove 3        # remove by index or path
uwu path list            # list entries (marks missing ones)
uwu path clean           # drop duplicates and dead entries
```
`-s/--system` targets the machine PATH (needs admin); `-f/--force` skips prompts.
Changes are written to the registry and broadcast to the system.

### `reload` — apply PATH/alias changes in place
```bash
uwu path add ./bin
uwu reload               # refresh PATH into the current session — like `source ~/.bashrc`
```
Requires `uwu init` (adds a PowerShell wrapper). No new terminal needed.

### `kill` — kill processes safely
```bash
uwu kill 3000            # kill whatever holds TCP port 3000 (a bare number = port)
uwu kill node            # by name, with globs: "*server", "chrome*", "py?hon"
uwu kill --pid 1234      # by PID
uwu kill node -n         # dry run — show what would die
uwu kill node -f         # skip the confirmation
```
Lists matches and confirms first. Refuses to kill critical system processes
(`csrss`, `lsass`, `wininit`, …) and never kills itself.

### `tree` — directory tree
```bash
uwu tree                 # honours .gitignore, hides dotfiles, prunes empty dirs
uwu tree src -L 2        # limit depth
uwu tree -a              # show everything (hidden + ignored)
uwu tree --ascii         # ASCII connectors instead of Unicode
uwu tree -d              # directories only
```
Directories and files are colour-coded.

### `shot` — screenshot
```bash
uwu shot                 # full screen -> uwu_screenshot_<timestamp>.png
uwu shot bug.png         # save to a specific file
uwu shot -c              # copy to the clipboard instead
uwu shot -w              # active window only
uwu shot -r              # interactive region select (Win+Shift+S style)
uwu shot -w -d 3         # wait 3s, then capture the window
```
Captured natively via GDI (fast, and not flagged by antivirus like PowerShell
screen-grab scripts are).

### `notify` — toast notification
```bash
uwu notify "build done"
uwu notify "ビルド完了 🎉" -t "cargo"   # Unicode & emoji supported
cargo build; uwu notify "finished"     # ping yourself when a long job ends
```

### `alias` — command aliases
```bash
uwu alias gs "git status"   # define
uwu alias gs                # show
uwu alias                   # list all
uwu alias gs -r             # remove
```
Aliases load into your shell via the `uwu init` profile; run `uwu reload` to apply.

### `web` — open a search or URL
```bash
uwu web rust lifetimes            # search
uwu web example.com               # open a URL
uwu web "rust book" -p bing       # pick a provider
```

### `wdex` — Windows Defender exclusions
```bash
uwu wdex add C:\dev          # exclude a path (self-elevates via UAC)
uwu wdex add node -p         # process exclusion
uwu wdex remove C:\dev
uwu wdex list
```

### `dns` — block domains and IPs
```bash
uwu dns block youtube.com     # sinkhole youtube.com *and* www.youtube.com
uwu dns block 1.2.3.4 10.0.0.0/8   # addresses/ranges go to the firewall
uwu dns block x.com -n        # dry run
uwu dns block x.com -e        # exact — skip the www. variant
uwu dns unblock youtube.com
uwu dns                       # list everything uwu blocks
uwu dns clear                 # remove every uwu-managed block
```
Domains are sinkholed (`0.0.0.0` **and** `::`, so IPv6 doesn't slip past) inside a
marker-fenced section of `hosts`; raw IPs get in/out firewall rules named
`uwu-block:<addr>`. Hand-written hosts entries and firewall rules are never
touched. Self-elevates via UAC and flushes the resolver cache afterwards.

### `del` — safe recursive delete
```bash
uwu del ./build          # confirms first
uwu del ./build -n       # dry run
uwu del ./build -f -v    # force, verbose
```
Guards against deleting protected/system paths.

### `init` — shell integration
Writes `~/.uwu/profile.ps1` and sources it from your `$PROFILE`, enabling
`uwu reload` and shell aliases. Re-run any time to refresh.

## Global flags

- `-q, --quiet` — suppress informational output (errors still show)
- `-h, --help` — help for any command (`uwu <cmd> --help`)
- `-V, --version` — print the version

## Design notes

- **Native, no PowerShell** — screen capture, process kill, and clipboard use the
  Win32 API directly. Faster, and immune to the AMSI false-positives that block
  PowerShell screen-grab scripts.
- **`.gitignore`-aware** — `tree` uses the same engine as ripgrep/fd.
- **Careful with destruction** — `del` and `kill` list what they'll touch,
  confirm by default (`-f` to skip), offer `-n` dry-run, and protect system
  paths/processes.
- **In-session updates** — `reload` refreshes PATH from the registry so you don't
  have to open a new terminal.

## License

[MIT](LICENSE) © rxxuzi
