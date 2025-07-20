use std::path::{Path, PathBuf};
use std::process::Command;
use anyhow::Result;
use tracing::debug;

/// Get the hooks directory for the repository, respecting core.hooksPath if set.
pub fn get_hooks_dir(repo: &git2::Repository) -> Result<PathBuf> {
    // Try to get core.hooksPath from config
    let config = repo.config()?;
    if let Ok(hooks_path) = config.get_string("core.hooksPath") {
        let hooks_path = PathBuf::from(hooks_path);
        if hooks_path.is_absolute() {
            Ok(hooks_path)
        } else {
            // Relative to repo root
            Ok(repo.path().parent().unwrap_or_else(|| Path::new(".")).join(hooks_path))
        }
    } else {
        // Default to .git/hooks
        Ok(repo.path().join("hooks"))
    }
}

/// Run a hook script if it exists and is executable. Returns Ok(true) if run, Ok(false) if not present.
pub fn run_hook(repo: &git2::Repository, hook_name: &str, args: &[&str]) -> Result<bool> {
    debug!("Looking for hook: {}", hook_name);
    let hooks_dir = get_hooks_dir(repo)?;
    let hook_path = hooks_dir.join(hook_name);
    // On Windows, allow .exe/.bat/.cmd as well as no extension
    #[cfg(windows)]
    let candidates = [
        hook_path.clone(),
        hook_path.with_extension("exe"),
        hook_path.with_extension("bat"),
        hook_path.with_extension("cmd"),
    ];
    #[cfg(not(windows))]
    let candidates = [hook_path.clone()];
    debug!("Hook candidates: {:?}", candidates);
    let hook = candidates.iter().find(|p| p.exists());
    if let Some(hook) = hook {
        debug!("Running hook: {}", hook.display());
        use std::process::Stdio;
        let mut cmd = Command::new(hook.clone());
        cmd.args(args);
        // Set environment variables as git does
        // TODO: Add more env vars if needed
        cmd.stdout(Stdio::inherit());
        cmd.stderr(Stdio::inherit());
        let status_result = cmd.status();
        match status_result {
            Ok(status) => {
                if !status.success() {
                    eprintln!("\n--- HOOK DEBUG ---");
                    eprintln!("Hook path: {:?}", hook);
                    eprintln!("Args: {:?}", args);
                    eprintln!("Exit code: {:?}", status.code());
                    eprintln!("--- END HOOK DEBUG ---\n");
                    anyhow::bail!("Hook {:?} failed with exit code {:?}", hook, status.code());
                }
                Ok(true)
            }
            Err(e) => {
                eprintln!("\n--- HOOK DEBUG ---");
                eprintln!("Hook path: {:?}", hook);
                eprintln!("Args: {:?}", args);
                eprintln!("Failed to spawn or wait for hook: {}", e);
                eprintln!("--- END HOOK DEBUG ---\n");
                Err(anyhow::anyhow!("Failed to run hook: {:?}: {}", hook, e))
            }
        }
    } else {
        debug!("No hook found for any candidate");
        Ok(false)
    }
}
