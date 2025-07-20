# `sup`: Trunk Based Development CLI tool

`sup` does `git stash && git pull && git stash pop` with recovery (`--abort` and `--continue`)

`sup` is a command-line tool for safely synchronizing your local git repository with its remote, even when you have uncommitted or untracked changes. It automates the process of stashing, pulling, and restoring your work, and provides robust handling for conflicts and interruptions.

## Features

- 🗃️ **Automatic stashing**: Stashes all local changes (including untracked files) before pulling.
- ⬇️ **Safe pull**: Runs a `git pull` (or equivalent) after stashing, then restores your changes from the stash.
- ⚔️ **Conflict handling**: If a conflict occurs, you can resolve it and use `sup --continue` to finish the operation.
- 🛑 **Abort support**: If you want to roll back, use `sup --abort` to restore your previous state and stashed changes.
- 📝 **State tracking**: Remembers interrupted operations and prevents accidental data loss.
- 🚀 **Commit and push**: Provide commit message with `--message/-m` flag to commit and push stashed changes, including hook support.

## Usage

```sh
sup                # Stash, pull, and restore changes
sup --continue     # Continue after resolving a conflict
sup --abort        # Abort and restore previous state
sup --message "Your commit message"  # Stash, pull, restore, and commit with a message
sup -m "Your commit message"  # Short form for --message
```

### Typical Workflow

1. Make local changes (even untracked files).
2. Run `sup` to pull from remote (optionally provide `--message/-m` to commit+push changes at the end):
    - Your changes are stashed.
    - The latest changes are pulled from the remote.
    - Your changes are reapplied, then optionally committed and pushed with the provided message.
3. If a conflict occurs:
    - Resolve the conflict in your files.
    - Stage the resolved files (`git add ...`).
    - Commit the resolution (`git commit -m "resolve conflict"`).
    - Run `sup --continue` to reapply your stashed changes and finish the operation (including optional commit+push)
4. If you want to cancel the operation:
    - Run `sup --abort` to restore your previous state and stashed changes.

## How It Works

- Stashes all local changes (tracked and untracked) with a special message.
- Pulls from the remote using either the git CLI or the `git2` library.
- Applies the stash back. If there are conflicts, the tool pauses and lets you resolve them.
- Tracks its state in `.git/sup_state` to allow safe abort/continue.

## Why Use sup?

- Enables most simplified git flow of Trunk Based Development.
- Avoids losing or overwriting local changes during a pull.
- Handles complex scenarios (conflicts, untracked files, interrupted pulls) automatically.
- Makes it easy to recover from mistakes or interruptions.

## Installation

### Windows with [Scoop](https://github.com/ScoopInstaller/Scoop)

```sh
scoop install https://raw.githubusercontent.com/strowk/sup/main/scoop/sup.json
```

, or if you already have it installed with scoop:

```sh
scoop update sup
```

### With bash script

In bash shell run:

```bash
curl -s https://raw.githubusercontent.com/strowk/sup/main/install.sh | bash
```

Should work in Linux bash, Windows Git Bash and MacOS.
For Windows users: you might need to start Git Bash from Administrator.

#### Disabling sudo

By default the script would try to install sup to `/usr/local/bin` and would require sudo rights for that,
but you can disable this behavior by setting `NO_SUDO` environment variable:

```bash
curl -s https://raw.githubusercontent.com/strowk/sup/main/install.sh | NO_SUDO=1 bash
```

Sudo is disabled by default for Windows Git Bash.

### Manually

Head to [latest release](https://github.com/strowk/sup/releases/latest), download archive for your OS/arch, unpack it and put binary somewhere in your PATH.

### From sources

If your system/architecture is not supported by the script above,
you can install Rust and install sup from sources:

```bash
git clone https://github.com/strowk/sup
cargo install --path ./sup
```
