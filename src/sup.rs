use crate::hooks;
use crate::ui::UI;
use anyhow::{Context, Result};
use git2::{ErrorCode, Repository, StashFlags};
use indicatif::ProgressStyle;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs::OpenOptions;
use std::fs::{self, File};
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::process::{self, exit};
use std::str::FromStr as _;
use tracing::instrument;
use tracing::Span;
use tracing::{debug, error, warn};
use tracing_indicatif::span_ext::IndicatifSpanExt;
use tracing_indicatif::IndicatifLayer;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use crate::serde::SupStateSerde;

const STATE_FILE: &str = ".git/sup_state";
const LOCK_FILE: &str = ".git/sup.lock";

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
#[serde(
    from = "SupStateSerde",
    into = "SupStateSerde",
    rename_all = "snake_case"
)]
pub(crate) enum SupState {
    Idle,
    InProgress {
        stash_created: bool,
        original_head: Option<String>,
        message: Option<String>,
    },
    Interrupted {
        stash_created: bool,
        stash_applied: bool,
        original_head: Option<String>,
        message: Option<String>,
    },
}

impl SupState {
    fn load() -> Result<Self> {
        let path = Path::new(STATE_FILE);
        if !path.exists() {
            return Ok(SupState::Idle);
        }
        let mut file = File::open(path)?;
        let mut buf = String::new();
        file.read_to_string(&mut buf)?;
        let state: SupState = serde_json::from_str(&buf)?;
        Ok(state)
    }
    fn save(&self) -> Result<()> {
        let mut file = File::create(STATE_FILE)?;
        let state_str = serde_json::to_string(self)?;
        file.write_all(state_str.as_bytes())?;
        Ok(())
    }
    fn clear() -> Result<()> {
        if Path::new(STATE_FILE).exists() {
            fs::remove_file(STATE_FILE)?;
        }
        Ok(())
    }
}

pub fn run_sup(
    r#continue: bool,
    abort: bool,
    version: bool,
    message: Option<String>,
    yes: bool,
    no_verify: bool,
) -> Result<()> {
    if version {
        println!("sup version {}", env!("CARGO_PKG_VERSION"));
        process::exit(0);
    }
    let indicatif_layer = IndicatifLayer::new().with_progress_style(
        ProgressStyle::with_template("{elapsed:>4.bold.dim} {spinner:.green} {wide_msg}  ")
            .expect("Failed to parse progress style"),
    );
    let targets = match env::var("RUST_LOG") {
        Ok(var) => Targets::from_str(&var)
            .map_err(|e| {
                eprintln!("Ignoring `RUST_LOG={var:?}`: {e}");
            })
            .unwrap_or_default(),
        Err(env::VarError::NotPresent) => {
            Targets::new().with_default(tracing_subscriber::FmtSubscriber::DEFAULT_MAX_LEVEL)
        }
        Err(e) => {
            eprintln!("Ignoring `RUST_LOG`: {e}");
            Targets::new().with_default(tracing_subscriber::FmtSubscriber::DEFAULT_MAX_LEVEL)
        }
    };

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_writer(indicatif_layer.get_stderr_writer()))
        .with(indicatif_layer)
        .with(targets)
        .init();

    // Acquire lock file to prevent concurrent sup runs
    let lock_path = Path::new(LOCK_FILE);
    let _lock_file = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(lock_path)
    {
        Ok(f) => f,
        Err(e) => {
            anyhow::bail!(
                "Another sup process is running, could not take a lock {}: {}. Aborting.",
                LOCK_FILE,
                e
            )
        }
    };
    // Ensure lock file is removed at the end (even on panic)
    struct LockGuard<'a> {
        path: &'a Path,
    }
    impl Drop for LockGuard<'_> {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(self.path);
        }
    }
    let _lock_guard = LockGuard { path: lock_path };

    ctrlc::set_handler(move || {
        let _ = std::fs::remove_file(lock_path);
        exit(1)
    })?;

    let mut state = SupState::load()?;
    if abort {
        let state_for_abort = state;
        match state_for_abort {
            SupState::Interrupted {
                stash_created,
                original_head,
                ..
            } => {
                let mut ui = UI::new();
                ui.log_abort();
                if let Some(ref orig_head) = original_head {
                    let mut repo =
                        Repository::open(".").context("failed to open git repository")?;
                    reset_repo(&mut ui, &mut repo, orig_head)?;
                }

                // Restore stashed changes if any
                if stash_created {
                    let mut repo =
                        Repository::open(".").context("failed to open git repository")?;
                    pop_stash(&mut ui, &mut repo);
                }
                ui.log_completed();
            }
            _ => {
                anyhow::bail!("No interrupted operation to abort");
            }
        }
        SupState::clear()?;
        return Ok(());
    }
    if r#continue {
        match state {
            SupState::Interrupted {
                stash_created,
                original_head,
                message,
                stash_applied,
            } => {
                let mut ui = UI::new();
                ui.log_continuing_interrupted_operation();

                // 1. If a merge is in progress, finish it (assume user resolved conflicts and staged files)
                let mut repo = Repository::open(".").context("failed to open git repository")?;
                if repo.state() == git2::RepositoryState::Merge {
                    merge_repo(&mut ui, &mut repo)?;
                }

                // 2. Apply stash if it was created
                if stash_created {
                    apply_stash_and_commit(
                        &mut repo,
                        stash_created,
                        stash_applied,
                        &original_head,
                        &message,
                        &mut ui,
                        yes,
                        no_verify,
                    )?;
                }
                SupState::clear()?;
                ui.log_completed();
                return Ok(());
            }
            _ => {
                anyhow::bail!("No interrupted operation to continue, {:?}", state);
            }
        }
    }
    if let SupState::InProgress { .. } = state {
        anyhow::bail!("Operation already in progress. To roll back, run with --abort. To continue, run with --continue.");
    }

    let mut ui = UI::new();
    let mut repo = Repository::open(".").context("failed to open git repository")?;

    let stash_created = stash_changes(&mut ui, &mut repo)?;

    let mut pulling = crate::pull::Pulling {};

    let original_head = pull_changes(&mut repo, &mut pulling, &mut ui, stash_created, &message)?;

    debug!("Checking out the head with force");
    // checking out the head to ensure that index and working directory are clean
    checking_out_with_force(&repo)?;
    // repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))?;
    state = SupState::InProgress {
        stash_created,
        original_head: original_head.clone(),
        message: message.clone(),
    };
    state.save()?;

    if stash_created {
        apply_stash_and_commit(
            &mut repo,
            stash_created,
            false,
            &original_head,
            &message,
            &mut ui,
            yes,
            no_verify,
        )?;
    }
    SupState::clear()?;
    ui.log_completed();
    // LockGuard will remove the lock file here
    Ok(())
}

#[instrument(skip_all)]
fn checking_out_with_force(repo: &Repository) -> Result<()> {
    Span::current().pb_set_message("Checking out HEAD with force");
    // checking out the head to ensure that index and working directory are clean
    repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))?;
    Ok(())
}

#[instrument(skip_all)]
fn merge_repo(ui: &mut UI, repo: &mut Repository) -> Result<()> {
    ui.configure_finishing_merge_progress(&Span::current());

    // Try to create a merge commit if index is not conflicted
    let mut index = repo.index()?;
    if index.has_conflicts() {
        anyhow::bail!(
            "Cannot continue: merge conflicts still present. Please resolve and stage all files."
        );
    }
    let sig = repo.signature()?;
    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let head_commit = repo.head()?.peel_to_commit()?;
    // Read .git/MERGE_HEAD to get merge parent OIDs
    let merge_head_path = Path::new(".git/MERGE_HEAD");
    let merge_head_content =
        std::fs::read_to_string(merge_head_path).context("Failed to read .git/MERGE_HEAD")?;
    // Collect parent commits as owned values
    let mut parent_commits = Vec::new();
    parent_commits.push(head_commit);
    for line in merge_head_content.lines() {
        let oid = git2::Oid::from_str(line.trim()).context("Invalid OID in MERGE_HEAD")?;
        let parent = repo.find_commit(oid)?;
        parent_commits.push(parent);
    }
    if parent_commits.len() < 2 {
        anyhow::bail!("No MERGE_HEAD found, cannot complete merge");
    }
    // Build refs vector for commit
    let parent_refs: Vec<&git2::Commit> = parent_commits.iter().collect();
    let msg = "Merge commit (sup --continue)";
    repo.commit(Some("HEAD"), &sig, &sig, msg, &tree, &parent_refs)?;
    repo.cleanup_state()?;
    debug!("Merge commit created and merge state cleaned up");
    Ok(())
}

#[instrument(skip_all)]
fn pop_stash(ui: &mut UI, repo: &mut Repository) {
    // Only pop the stash created by sup (with message 'sup stash')
    ui.configure_restoring_stashed_changes_for_abort_progress(&Span::current());
    let mut sup_stash_index: Option<usize> = None;
    let mut idx = 0;
    let _ = repo.stash_foreach(|stash_index, stash_msg, _| {
        if stash_index > 0 {
            return false; // only last stash can be correct sup stash
        }
        if stash_msg.ends_with("sup stash") {
            sup_stash_index = Some(stash_index);
            return false; // stop after finding
        }
        warn!("Ignoring unrecognized stash {}: {}", idx, stash_msg);
        idx += 1;
        true
    });
    if let Some(stash_index) = sup_stash_index {
        match repo.stash_pop(stash_index, None) {
            Ok(_) => debug!("sup stash applied during abort"),
            Err(e) => error!("Failed to apply sup stash during abort: {}", e),
        }
    } else {
        warn!("No sup stash found to apply during abort; likely already popped or not created");
    }
}

#[instrument(skip_all)]
fn reset_repo(ui: &mut UI, repo: &mut Repository, orig_head: &str) -> Result<()> {
    ui.configure_resetting_progress(&Span::current(), orig_head);
    repo.reset(
        &repo.find_object(
            git2::Oid::from_str(orig_head)?,
            Some(git2::ObjectType::Commit),
        )?,
        git2::ResetType::Hard,
        None,
    )?;
    Ok(())
}

#[instrument(skip_all)]
fn check_conflicts(repo: &Repository) -> Result<bool> {
    Span::current().pb_set_message("Checking for conflicts");
    let statuses = repo.statuses(None)?;
    for entry in statuses.iter() {
        if entry.status().is_conflicted() {
            return Ok(true);
        }
    }
    Ok(false)
}

#[instrument(skip_all)]
fn pull_changes(
    repo: &mut Repository,
    pulling: &mut crate::pull::Pulling,
    ui: &mut UI,
    stash_created: bool,
    message: &Option<String>,
) -> Result<Option<String>> {
    ui.configure_pulling_progress(&Span::current());

    let original_head = Some(repo.head()?.target().map(|oid| oid.to_string())).flatten();
    if std::env::var("PULL_WITH_CLI").is_ok() {
        let status = std::process::Command::new("git").arg("pull").status()?;
        if !status.success() {
            error!("git pull failed");
            SupState::Interrupted {
                stash_created,
                original_head,
                message: message.clone(),
                stash_applied: false,
            }
            .save()?;
            anyhow::bail!("git pull failed");
        }
    } else {
        // Determine current branch
        let head = repo.head()?;
        let branch = if head.is_branch() {
            head.shorthand().map(|s| s.to_string())
        } else {
            None
        };
        // Determine remote for current branch
        let remote = if let Some(ref branch_name) = branch {
            let branch_ref = repo.find_branch(branch_name, git2::BranchType::Local)?;
            branch_ref.upstream().ok().and_then(|up| {
                match up.name() {
                    Ok(Some(name)) => {
                        // name is like "refs/remotes/origin/master"
                        let parts: Vec<&str> = name.split('/').collect();
                        if parts.len() >= 3 {
                            Some(parts[2].to_string())
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            })
        } else {
            None
        };
        let args = crate::pull::Args {
            arg_remote: remote,
            arg_branch: branch,
        };
        if let Err(e) = pulling.pull_run(&args) {
            error!("git pull failed: {}", e);
            SupState::Interrupted {
                stash_created,
                original_head,
                message: message.clone(),
                stash_applied: false,
            }
            .save()?;
            anyhow::bail!("git pull failed: {e}");
        }
    }

    Ok(original_head)
}

#[instrument(skip_all)]
fn stash_changes(ui: &mut UI, repo: &mut Repository) -> Result<bool, anyhow::Error> {
    ui.configure_stashing_progress(&Span::current());
    let sig = repo.signature()?;
    let stash_result = repo.stash_save(&sig, "sup stash", Some(StashFlags::INCLUDE_UNTRACKED));
    let stash_created = match stash_result {
        Ok(_) => true,
        Err(ref e) if e.code() == ErrorCode::NotFound => {
            debug!("No changes to stash");
            false
        }
        Err(e) => return Err(e.into()),
    };
    Ok(stash_created)
}

#[instrument(skip_all)]
fn apply_stash(repo: &mut Repository, ui: &mut UI) -> Result<()> {
    ui.configure_applying_stash_progress(&Span::current());
    // Ensure index is clean before applying stashed changes
    repo.reset(
        repo.head()?.peel_to_commit()?.as_object(),
        git2::ResetType::Mixed,
        None,
    )?;
    // Use stash_apply and only drop if no conflicts
    let apply_res = repo.stash_apply(0, None);
    apply_res?;
    Ok(())
}

fn apply_stash_and_commit(
    repo: &mut Repository,
    stash_created: bool,
    stash_applied: bool,
    original_head: &Option<String>,
    message: &Option<String>,
    ui: &mut UI,
    yes: bool,
    no_verify: bool,
) -> Result<(), anyhow::Error> {
    if stash_applied {
        let has_conflicts = check_conflicts(repo)?;
        if has_conflicts {
            error!("Conflicts detected before dropping stash");
            anyhow::bail!("Conflicts detected, cannot continue");
        }
        // If --message/-m is provided, stage and commit all changes
        stage_and_commit_with_hooks(repo, message, ui, no_verify)?;

        if yes {
            debug!("Dropping stash entry since stash was applied previously");
            repo.stash_drop(0)?;
            return Ok(());
        }
        let res = dialoguer::Confirm::new()
            .with_prompt("Stash was already applied, do you want to drop it?")
            .default(!yes)
            .interact()?;
        if res {
            debug!("Dropping stash entry since stash was applied previously");
            repo.stash_drop(0)?;
            return Ok(());
        }
        return Ok(());
    }
    let apply_res = apply_stash(repo, ui);
    match apply_res {
        Ok(_) => {
            debug!("Stash applied, checking for conflicts");
            let has_conflicts = check_conflicts(repo)?;
            if has_conflicts {
                error!("Conflicts detected after stash apply");
                SupState::Interrupted {
                    stash_created,
                    original_head: original_head.clone(),
                    message: message.clone(),
                    stash_applied: true,
                }
                .save()?;
                anyhow::bail!("Conflicts detected after stash apply");
            } else {
                debug!("Stash applied successfully with no conflicts");
                // If --message/-m is provided, stage and commit all changes
                stage_and_commit_with_hooks(repo, message, ui, no_verify)?;
                debug!("Dropping stash entry after successful apply");
                repo.stash_drop(0)?;
            }
        }
        Err(e) => {
            error!("Failed to apply stash: {}", e);
            SupState::Interrupted {
                stash_created,
                original_head: original_head.clone(),
                message: message.clone(),
                stash_applied: true,
            }
            .save()?;
            anyhow::bail!("Failed to apply stash");
        }
    };
    Ok(())
}

#[instrument(skip_all)]
fn commit_stashed_changes(
    ui: &mut UI,
    repo: &Repository,
    msg: &str,
    no_verify: bool,
) -> Result<()> {
    ui.configure_committing_stashed_changes_progress_bar(&Span::current());
    if !no_verify { // --no-verify skips pre-commit hook
        // Run pre-commit hook if present, must suspend progress bar
        if let Err(e) = hooks::run_hook(repo, "pre-commit", &[]) {
            error!("pre-commit hook failed: {}", e);
            SupState::Idle.save()?;
            return Err(e);
        }
    }
    // Prepare commit message file for commit-msg hook
    let mut commit_msg_file = tempfile::NamedTempFile::new()?;
    commit_msg_file.write_all(msg.as_bytes())?;
    let commit_msg_path = commit_msg_file.path().to_str().unwrap();
    if let Err(e) = hooks::run_hook(repo, "commit-msg", &[commit_msg_path]) {
        error!("commit-msg hook failed: {}", e);
        SupState::Idle.save()?;
        return Err(e);
    }

    let mut index = repo.index()?;
    index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)?;
    index.write()?;
    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let sig = repo.signature()?;
    let parent_commit = repo.head()?.peel_to_commit()?;
    repo.commit(Some("HEAD"), &sig, &sig, msg, &tree, &[&parent_commit])?;
    Ok(())
}

#[instrument(skip_all)]
fn push_committed_changes(ui: &mut UI, repo: &Repository, no_verify: bool) -> Result<()> {
    // Push the current branch using libgit2
    let head = repo.head()?;
    if let Some(branch) = head.shorthand() {
        ui.configure_pushing_progress(&Span::current(), branch);
        if let Err(e) = push(repo, branch, no_verify) {
            error!("Failed to push branch '{}': {}", branch, e);
            SupState::Idle.save()?;
            return Err(e);
        }
    }
    Ok(())
}

fn stage_and_commit_with_hooks(
    repo: &Repository,
    message: &Option<String>,
    ui: &mut UI,
    no_verify: bool,
) -> Result<(), anyhow::Error> {
    if let Some(ref msg) = message {
        commit_stashed_changes(ui, repo, msg, no_verify)?;
        // Push the current branch using libgit2
        push_committed_changes(ui, repo, no_verify)?;
    }
    Ok(())
}

fn push(repo: &Repository, branch: &str, no_verify: bool) -> anyhow::Result<()> {
    // Run pre-push hook if present
    if !no_verify { // --no-verify skips pre-push hook
        hooks::run_hook(repo, "pre-push", &["origin"])?;
    }
    let mut remote = repo.find_remote("origin")?;
    let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");
    let mut callbacks = git2::RemoteCallbacks::new();
    callbacks.credentials(|url, username_from_url, allowed_types| {
        crate::credentials::callback(url, username_from_url, &allowed_types, repo)
    });
    let mut push_options = git2::PushOptions::new();
    push_options.remote_callbacks(callbacks);
    remote
        .push(&[&refspec], Some(&mut push_options))
        .map_err(|e| {
            error!("libgit2 push failed: {}", e);
            anyhow::anyhow!("libgit2 push failed: {}", e)
        })?;

    Ok(())
}

