use crate::hooks;
use anyhow::{Context, Result};
use console::{style, Emoji};
use git2::{ErrorCode, Repository, StashFlags};
use indicatif::{ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::fs::{self, File};
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::process;
use std::time::Duration;
use tracing::{debug, error, warn};

const STATE_FILE: &str = ".git/sup_state";
const LOCK_FILE: &str = ".git/sup.lock";

static FLOPPY_DISK: Emoji<'_, '_> = Emoji("🗃️  ", "");
static DOWN_ARROW: Emoji<'_, '_> = Emoji("🔽  ", "");
static ROCKET: Emoji<'_, '_> = Emoji("🚀 ", "");
static CHECKMARK: Emoji<'_, '_> = Emoji("✅  ", "");
static BOX: Emoji<'_, '_> = Emoji("📦  ", "");
static RELOAD: Emoji<'_, '_> = Emoji("🔄  ", "");

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
#[serde(
    from = "SupStateSerde",
    into = "SupStateSerde",
    rename_all = "snake_case"
)]
enum SupState {
    Idle,
    InProgress {
        stash_created: bool,
        original_head: Option<String>,
        message: Option<String>,
    },
    Interrupted {
        stash_created: bool,
        original_head: Option<String>,
        message: Option<String>,
    },
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
enum SupStateSerde {
    Idle,
    InProgress(bool, Option<String>, Option<String>),
    Interrupted(bool, Option<String>, Option<String>),
}

impl From<SupState> for SupStateSerde {
    fn from(state: SupState) -> Self {
        match state {
            SupState::Idle => SupStateSerde::Idle,
            SupState::InProgress {
                stash_created,
                original_head,
                message,
            } => SupStateSerde::InProgress(stash_created, original_head, message),
            SupState::Interrupted {
                stash_created,
                original_head,
                message,
            } => SupStateSerde::Interrupted(stash_created, original_head, message),
        }
    }
}

impl From<SupStateSerde> for SupState {
    fn from(state: SupStateSerde) -> Self {
        match state {
            SupStateSerde::Idle => SupState::Idle,
            SupStateSerde::InProgress(stash_created, original_head, message) => {
                SupState::InProgress {
                    stash_created,
                    original_head,
                    message,
                }
            }
            SupStateSerde::Interrupted(stash_created, original_head, message) => {
                SupState::Interrupted {
                    stash_created,
                    original_head,
                    message,
                }
            }
        }
    }
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
) -> Result<()> {
    if version {
        println!("sup version {}", env!("CARGO_PKG_VERSION"));
        process::exit(0);
    }
    tracing_subscriber::fmt::init();
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
    let mut state = SupState::load()?;
    if abort {
        let state_for_abort = state;
        match state_for_abort {
            SupState::Interrupted {
                stash_created,
                original_head,
                ..
            } => {
                let mut steps_total = 1;
                if original_head.is_some() {
                    steps_total += 1;
                }
                if stash_created {
                    steps_total += 1;
                }
                let mut ui = UI::new(steps_total);
                ui.log_abort();
                if let Some(ref orig_head) = original_head {
                    let repo = Repository::open(".").context("failed to open git repository")?;
                    let resetting_progress = ui.get_reset_progress(orig_head);
                    repo.reset(
                        &repo.find_object(
                            git2::Oid::from_str(orig_head)?,
                            Some(git2::ObjectType::Commit),
                        )?,
                        git2::ResetType::Hard,
                        None,
                    )?;
                    ui.finish_reset_progress(resetting_progress, orig_head);
                }

                // Restore stashed changes if any
                if stash_created {
                    let mut repo =
                        Repository::open(".").context("failed to open git repository")?;
                    // Only pop the stash created by sup (with message 'sup stash')
                    let restoring_progress = ui.get_restoring_stashed_changes_for_abort_progress();
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
                    ui.finish_restoring_stashed_changes_for_abort(restoring_progress);
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
            } => {
                let mut steps_total = 1;
                if original_head.is_some() {
                    steps_total += 1;
                }
                if stash_created {
                    steps_total += 1;
                    if message.is_some() {
                        steps_total += 2; // commit and push
                    }
                }
                let mut ui = UI::new(steps_total);
                ui.log_continuing_interrupted_operation();

                // 1. If a merge is in progress, finish it (assume user resolved conflicts and staged files)
                let mut repo = Repository::open(".").context("failed to open git repository")?;
                if repo.state() == git2::RepositoryState::Merge {
                    let progress = ui.get_finishing_merge_progress();

                    // Try to create a merge commit if index is not conflicted
                    let mut index = repo.index()?;
                    if index.has_conflicts() {
                        anyhow::bail!("Cannot continue: merge conflicts still present. Please resolve and stage all files.");
                    }
                    let sig = repo.signature()?;
                    let tree_id = index.write_tree()?;
                    let tree = repo.find_tree(tree_id)?;
                    let head_commit = repo.head()?.peel_to_commit()?;
                    // Read .git/MERGE_HEAD to get merge parent OIDs
                    let merge_head_path = Path::new(".git/MERGE_HEAD");
                    let merge_head_content = std::fs::read_to_string(merge_head_path)
                        .context("Failed to read .git/MERGE_HEAD")?;
                    // Collect parent commits as owned values
                    let mut parent_commits = Vec::new();
                    parent_commits.push(head_commit);
                    for line in merge_head_content.lines() {
                        let oid = git2::Oid::from_str(line.trim())
                            .context("Invalid OID in MERGE_HEAD")?;
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
                    ui.finish_finishing_merge_progress(progress);
                }
                // 2. Apply stash if it was created
                if stash_created {
                    let applying_stash_progress = ui.get_applying_stash_progress_bar();
                    // Ensure index is clean before applying stash
                    repo.reset(
                        repo.head()?.peel_to_commit()?.as_object(),
                        git2::ResetType::Mixed,
                        None,
                    )?;
                    // Use stash_apply and only drop if no conflicts
                    let stash_apply_result: std::result::Result<(), git2::Error> =
                        repo.stash_apply(0, None);
                    ui.finish_applying_stash_progress(applying_stash_progress);
                    match stash_apply_result {
                        Ok(_) => {
                            debug!("Stash applied");
                            let mut has_conflicts = false;
                            {
                                let statuses = repo.statuses(None)?;
                                for entry in statuses.iter() {
                                    let s = entry.status();
                                    if s.is_conflicted() {
                                        has_conflicts = true;
                                        break;
                                    }
                                }
                            }
                            if has_conflicts {
                                error!("Conflicts detected after stash apply");
                                state = SupState::Interrupted {
                                    stash_created,
                                    original_head: original_head.clone(),
                                    message: message.clone(),
                                };
                                state.save()?;
                                anyhow::bail!("Conflicts detected after stash apply");
                            } else {
                                // If message is present, stage and commit all changes
                                if let Some(ref msg) = message {
                                    let committing_progress =
                                        ui.get_committing_stashed_changes_progress_bar();
                                    let mut index = repo.index()?;
                                    index.add_all(
                                        ["*"].iter(),
                                        git2::IndexAddOption::DEFAULT,
                                        None,
                                    )?;
                                    index.write()?;
                                    let tree_id = index.write_tree()?;
                                    let tree = repo.find_tree(tree_id)?;
                                    let sig = repo.signature()?;
                                    let parent_commit = repo.head()?.peel_to_commit()?;
                                    // Run pre-commit hook if present
                                    if let Err(e) = hooks::run_hook(&repo, "pre-commit", &[]) {
                                        error!("pre-commit hook failed: {}", e);
                                        state = SupState::Idle;
                                        state.save()?;
                                        return Err(e);
                                    }
                                    // Prepare commit message file for commit-msg hook
                                    let mut commit_msg_file = tempfile::NamedTempFile::new()?;
                                    commit_msg_file.write_all(msg.as_bytes())?;
                                    let commit_msg_path = commit_msg_file.path().to_str().unwrap();
                                    if let Err(e) =
                                        hooks::run_hook(&repo, "commit-msg", &[commit_msg_path])
                                    {
                                        error!("commit-msg hook failed: {}", e);
                                        state = SupState::Idle;
                                        state.save()?;
                                        return Err(e);
                                    }
                                    repo.commit(
                                        Some("HEAD"),
                                        &sig,
                                        &sig,
                                        msg,
                                        &tree,
                                        &[&parent_commit],
                                    )?;
                                    ui.finish_committing_stashed_changes(committing_progress);
                                    // Push the current branch using libgit2
                                    let head = repo.head()?;
                                    if let Some(branch) = head.shorthand() {
                                        let pushing_progress = ui.get_pushing_progress_bar(branch);
                                        if let Err(e) = push(&repo, branch) {
                                            error!("Failed to push branch '{}': {}", branch, e);
                                            state = SupState::Idle;
                                            state.save()?;
                                            return Err(e);
                                        }
                                        ui.finish_pushing_progress(pushing_progress, branch);
                                    }
                                }
                                debug!("Dropping stash entry after successful apply");
                                repo.stash_drop(0)?;
                            }
                        }
                        Err(e) => {
                            error!("Failed to apply stash: {}", e);
                            state = SupState::Interrupted {
                                stash_created,
                                original_head: original_head.clone(),
                                message: message.clone(),
                            };
                            state.save()?;
                            anyhow::bail!("Failed to apply stash");
                        }
                    }
                }
                SupState::clear()?;
                ui.log_completed();
                return Ok(());
            }
            _ => {
                anyhow::bail!("No interrupted operation to continue");
            }
        }
    }
    if let SupState::InProgress { .. } = state {
        anyhow::bail!("Operation already in progress. To roll back, run with --abort. To continue, run with --continue.");
    }

    let mut ui = UI::new(if message.is_some() { 5 } else { 3 });
    let mut repo = Repository::open(".").context("failed to open git repository")?;

    let stashing_progress = ui.get_stashing_progress_bar();

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

    // stashing_progress.finish_with_message(format!("{}Stashed local changes", FLOPPY_DISK));
    ui.finish_stashing_progress(stashing_progress);

    let mut pulling = crate::pull::Pulling {
        multi_progress: indicatif::MultiProgress::new(),
    };

    let pull_progress = ui.get_pulling_progress_bar(&mut pulling);

    let original_head = Some(repo.head()?.target().map(|oid| oid.to_string())).flatten();
    if std::env::var("PULL_WITH_CLI").is_ok() {
        let status = std::process::Command::new("git").arg("pull").status()?;
        if !status.success() {
            error!("git pull failed");
            state = SupState::Interrupted {
                stash_created,
                original_head,
                message: message.clone(),
            };
            state.save()?;
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
            state = SupState::Interrupted {
                stash_created,
                original_head,
                message: message.clone(),
            };
            state.save()?;
            anyhow::bail!("git pull failed: {e}");
        }
    }
    ui.finish_pulling_progress(pull_progress);

    debug!("Checking out the head with force");
    // checking out the head to ensure that index and working directory are clean
    repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))?;
    state = SupState::InProgress {
        stash_created,
        original_head: original_head.clone(),
        message: message.clone(),
    };
    state.save()?;

    if stash_created {
        // Ensure index is clean before applying stashed changes
        repo.reset(
            repo.head()?.peel_to_commit()?.as_object(),
            git2::ResetType::Mixed,
            None,
        )?;
        let applying_stash_progress = ui.get_applying_stash_progress_bar();
        let apply_res = repo.stash_apply(0, None);
        ui.finish_applying_stash_progress(applying_stash_progress);
        match apply_res {
            Ok(_) => {
                debug!("Stash applied, checking for conflicts");
                // Check for conflicts after stash pop
                let mut has_conflicts = false;
                let statuses = repo.statuses(None)?;
                for entry in statuses.iter() {
                    let s = entry.status();
                    if s.is_conflicted() {
                        has_conflicts = true;
                        break;
                    }
                }
                if has_conflicts {
                    error!("Conflicts detected after stash apply");
                    state = SupState::Interrupted {
                        stash_created,
                        original_head,
                        message: message.clone(),
                    };
                    state.save()?;
                    anyhow::bail!("Conflicts detected after stash apply");
                } else {
                    debug!("Stash applied successfully with no conflicts");
                    // If --message/-m is provided, stage and commit all changes
                    if let Some(ref msg) = message {
                        let committing_progress = ui.get_committing_stashed_changes_progress_bar();
                        // Run pre-commit hook if present
                        if let Err(e) = hooks::run_hook(&repo, "pre-commit", &[]) {
                            error!("pre-commit hook failed: {}", e);
                            state = SupState::Idle;
                            state.save()?;
                            return Err(e);
                        }
                        // Prepare commit message file for commit-msg hook
                        let mut commit_msg_file = tempfile::NamedTempFile::new()?;
                        commit_msg_file.write_all(msg.as_bytes())?;
                        let commit_msg_path = commit_msg_file.path().to_str().unwrap();
                        if let Err(e) = hooks::run_hook(&repo, "commit-msg", &[commit_msg_path]) {
                            error!("commit-msg hook failed: {}", e);
                            state = SupState::Idle;
                            state.save()?;
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
                        ui.finish_committing_stashed_changes(committing_progress);
                        // Push the current branch using libgit2
                        let head = repo.head()?;
                        if let Some(branch) = head.shorthand() {
                            let pushing_progress = ui.get_pushing_progress_bar(branch);
                            if let Err(e) = push(&repo, branch) {
                                error!("Failed to push branch '{}': {}", branch, e);
                                state = SupState::Idle;
                                state.save()?;
                                return Err(e);
                            }
                            ui.finish_pushing_progress(pushing_progress, branch);
                        }
                    }
                }
            }
            Err(e) => {
                error!("Failed to apply stash: {}", e);
                state = SupState::Interrupted {
                    stash_created,
                    original_head,
                    message: message.clone(),
                };
                state.save()?;
                anyhow::bail!("Failed to apply stash");
            }
        }
        debug!("Dropping stash entry");
        repo.stash_drop(0)?;
    }
    ui.log_completed();
    SupState::clear()?;
    // LockGuard will remove the lock file here
    Ok(())
}

fn push(repo: &Repository, branch: &str) -> anyhow::Result<()> {
    // Run pre-push hook if present
    hooks::run_hook(repo, "pre-push", &["origin"])?;
    let mut remote = repo.find_remote("origin")?;
    let refspec = format!("refs/heads/{}:refs/heads/{}", branch, branch);
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

struct UI {
    steps_count: usize,
    total_steps: usize,
}

impl UI {
    fn new(total_steps: usize) -> Self {
        UI {
            steps_count: 1,
            total_steps,
        }
    }

    fn next_step(&mut self) {
        self.steps_count += 1;
    }

    fn log_completed(&self) {
        println!("        {}Operation completed", CHECKMARK);
    }

    fn get_stashing_progress_bar(&mut self) -> ProgressBar {
        let progress = ProgressBar::new_spinner();
        progress.set_message("Stashing local changes");
        progress.enable_steady_tick(Duration::from_millis(100));
        progress.set_style(
            ProgressStyle::with_template("{prefix:.bold.dim} {spinner:.green} {wide_msg} :{elapsed}  ")
                .expect("Failed to parse stashing progress style"),
        );
        progress.set_prefix(format!("[{}/{}]", self.steps_count, self.total_steps));
        self.next_step();
        progress
    }

    fn finish_stashing_progress(&self, progress: ProgressBar) {
        progress.finish_with_message(format!("{}Stashed local changes", FLOPPY_DISK));
    }

    fn get_applying_stash_progress_bar(&mut self) -> ProgressBar {
        let progress = ProgressBar::new_spinner();
        progress.set_message("Applying stashed changes");
        progress.enable_steady_tick(Duration::from_millis(100));
        progress.set_style(
            ProgressStyle::with_template("{prefix:.bold.dim} {spinner:.green} {wide_msg} :{elapsed}  ")
                .expect("Failed to parse applying stash progress style"),
        );
        progress.set_prefix(format!("[{}/{}]", self.steps_count, self.total_steps));
        self.next_step();
        progress
    }

    fn finish_applying_stash_progress(&self, progress: ProgressBar) {
        progress.finish_with_message(format!("{}Applied stashed changes", BOX));
    }

    fn get_pulling_progress_bar(&mut self, pulling: &mut crate::pull::Pulling) -> ProgressBar {
        let progress = pulling.multi_progress.add(ProgressBar::new_spinner());
        progress.set_message("Pulling remote changes");
        progress.enable_steady_tick(Duration::from_millis(100));
        progress.set_style(
            ProgressStyle::with_template("{prefix:.bold.dim} {spinner:.green} {wide_msg} :{elapsed}  ")
                .expect("Failed to parse pulling progress style"),
        );
        progress.set_prefix(format!("[{}/{}]", self.steps_count, self.total_steps));
        self.next_step();
        progress
    }

    fn finish_pulling_progress(&self, progress: ProgressBar) {
        progress.finish_with_message(format!("{}Pulled remote changes", DOWN_ARROW));
    }

    fn log_abort(&mut self) {
        println!(
            "{} {}Aborting and rolling back operation",
            style(format!("[{}/{}]", self.steps_count, self.total_steps))
                .bold()
                .dim(),
            RELOAD,
        );
        self.next_step();
    }

    fn get_reset_progress(&mut self, orig_head: &str) -> ProgressBar {
        let progress = ProgressBar::new_spinner();
        progress.set_message(format!(
            "Resetting branch to original commit before pull: {}",
            orig_head
        ));
        progress.enable_steady_tick(Duration::from_millis(100));
        progress.set_style(
            ProgressStyle::with_template("{prefix:.bold.dim} {spinner:.green} {wide_msg} :{elapsed}  ")
                .expect("Failed to parse reset progress style"),
        );
        progress.set_prefix(format!("[{}/{}]", self.steps_count, self.total_steps));
        self.next_step();
        progress
    }

    fn finish_reset_progress(&self, progress: ProgressBar, orig_head: &str) {
        progress.finish_with_message(format!(
            "{}Reset branch to commit before pull: {}",
            FLOPPY_DISK, orig_head
        ));
    }

    fn get_restoring_stashed_changes_for_abort_progress(&mut self) -> ProgressBar {
        let progress = ProgressBar::new_spinner();
        progress.set_message("Restoring stashed changes after abort");
        progress.enable_steady_tick(Duration::from_millis(100));
        progress.set_style(
            ProgressStyle::with_template("{prefix:.bold.dim} {spinner:.green} {wide_msg} :{elapsed}  ")
                .expect("Failed to parse restoring stashed changes progress style"),
        );
        progress.set_prefix(format!("[{}/{}]", self.steps_count, self.total_steps));
        self.next_step();
        progress
    }

    fn finish_restoring_stashed_changes_for_abort(&self, progress: ProgressBar) {
        progress.finish_with_message(format!("{}Restored stashed changes", BOX));
    }

    fn get_committing_stashed_changes_progress_bar(&mut self) -> ProgressBar {
        let progress = ProgressBar::new_spinner();
        progress.set_message("Committing stashed changes");
        progress.enable_steady_tick(Duration::from_millis(100));
        progress.set_style(
            ProgressStyle::with_template("{prefix:.bold.dim} {spinner:.green} {wide_msg} :{elapsed}  ")
                .expect("Failed to parse committing stashed changes progress style"),
        );
        progress.set_prefix(format!("[{}/{}]", self.steps_count, self.total_steps));
        self.next_step();
        progress
    }

    fn finish_committing_stashed_changes(&self, progress: ProgressBar) {
        progress.finish_with_message(format!("{}Committed stashed changes", CHECKMARK));
    }

    fn get_pushing_progress_bar(&mut self, branch: &str) -> ProgressBar {
        let progress = ProgressBar::new_spinner();
        progress.set_message(format!("Pushing branch '{}'", branch));
        progress.enable_steady_tick(Duration::from_millis(100));
        progress.set_style(
            ProgressStyle::with_template("{prefix:.bold.dim} {spinner:.green} {wide_msg} :{elapsed}  ")
                .expect("Failed to parse pushing progress style"),
        );
        progress.set_prefix(format!("[{}/{}]", self.steps_count, self.total_steps));
        self.next_step();
        progress
    }

    fn finish_pushing_progress(&self, progress: ProgressBar, branch: &str) {
        progress.finish_with_message(format!("{}Pushed branch '{}'", ROCKET, branch));
    }

    fn log_continuing_interrupted_operation(&self) {
        println!(
            "{} {}Continuing interrupted operation",
            style(format!("[{}/{}]", self.steps_count, self.total_steps))
                .bold()
                .dim(),
            RELOAD
        );
    }

    fn get_finishing_merge_progress(&mut self) -> ProgressBar {
        let progress: ProgressBar = ProgressBar::new_spinner();
        progress.set_message("Finishing merge in progress (creating merge commit)");
        progress.enable_steady_tick(Duration::from_millis(100));
        progress.set_style(
            ProgressStyle::with_template("{prefix:.bold.dim} {spinner:.green} {wide_msg} :{elapsed}  ")
                .expect("Failed to parse finishing merge progress style"),
        );
        progress.set_prefix(format!("[{}/{}]", self.steps_count, self.total_steps));
        self.next_step();
        progress
    }

    fn finish_finishing_merge_progress(&self, progress: ProgressBar) {
        progress.finish_with_message(format!("{}Finished merge commit", FLOPPY_DISK));
    }
}
