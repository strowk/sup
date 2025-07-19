# sup: Safe Git Pull with Stash Management

`sup` is a command-line tool for safely synchronizing your local git repository with its remote, even when you have uncommitted or untracked changes. It automates the process of stashing, pulling, and restoring your work, and provides robust handling for conflicts and interruptions.

## Features

- **Automatic stashing**: Stashes all local changes (including untracked files) before pulling.
- **Safe pull**: Runs a `git pull` (or equivalent) after stashing, then restores your changes from the stash.
- **Conflict handling**: If a conflict occurs, you can resolve it and use `sup --continue` to finish the operation.
- **Abort support**: If you want to roll back, use `sup --abort` to restore your previous state and stashed changes.
- **State tracking**: Remembers interrupted operations and prevents accidental data loss.

## Usage

```sh
sup                # Stash, pull, and restore changes
sup --continue     # Continue after resolving a conflict
sup --abort        # Abort and restore previous state
```

### Typical Workflow

1. Make local changes (even untracked files).
2. Run `sup` to pull from remote:
    - Your changes are stashed.
    - The latest changes are pulled from the remote.
    - Your changes are reapplied.
3. If a conflict occurs:
    - Resolve the conflict in your files.
    - Stage the resolved files (`git add ...`).
    - Commit the resolution (`git commit -m "resolve conflict"`).
    - Run `sup --continue` to reapply your stashed changes.
4. If you want to cancel the operation:
    - Run `sup --abort` to restore your previous state and stashed changes.

## How It Works

- Stashes all local changes (tracked and untracked) with a special message.
- Pulls from the remote using either the git CLI or the `git2` library.
- Applies the stash back. If there are conflicts, the tool pauses and lets you resolve them.
- Tracks its state in `.git/sup_state` to allow safe abort/continue.

## Why Use sup?

- Avoids losing or overwriting local changes during a pull.
- Handles complex scenarios (conflicts, untracked files, interrupted pulls) automatically.
- Makes it easy to recover from mistakes or interruptions.
