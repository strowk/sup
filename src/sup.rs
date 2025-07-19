use anyhow::{Context, Result};
use git2::{ErrorCode, Repository, StashFlags};
use serde::{Deserialize, Serialize};
use serde_json;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use tracing::{error, info, warn};

const STATE_FILE: &str = ".git/sup_state";

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
#[serde(from = "SupStateSerde", into = "SupStateSerde", rename_all = "snake_case")]
enum SupState {
    Idle,
    InProgress {
        stash_created: bool,
        original_head: Option<String>,
    },
    Interrupted {
        stash_created: bool,
        original_head: Option<String>,
    },
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
enum SupStateSerde {
    Idle,
    InProgress (
         bool,
         Option<String>,
    ),
    Interrupted (
        bool,
        Option<String>,
    ),
}

impl From<SupState> for SupStateSerde {
    fn from(state: SupState) -> Self {
        match state {
            SupState::Idle => SupStateSerde::Idle,
            SupState::InProgress { stash_created, original_head } => {
                SupStateSerde::InProgress(stash_created, original_head)
            }
            SupState::Interrupted { stash_created, original_head } => {
                SupStateSerde::Interrupted(stash_created, original_head)
            }
        }
    }
}

impl From<SupStateSerde> for SupState {
    fn from(state: SupStateSerde) -> Self {
        match state {
            SupStateSerde::Idle => SupState::Idle,
            SupStateSerde::InProgress(stash_created, original_head) => {
                SupState::InProgress { stash_created, original_head }
            }
            SupStateSerde::Interrupted(stash_created, original_head) => {
                SupState::Interrupted { stash_created, original_head }
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

pub fn run_sup(r#continue: bool, abort: bool) -> Result<()> {
    tracing_subscriber::fmt::init();
    let mut state = SupState::load()?;
    if abort {
        info!("Aborting and rolling back operation");
        let state_for_abort = state;
        match state_for_abort {
            SupState::Interrupted {
                stash_created,
                original_head,
            } => {
                if let Some(ref orig_head) = original_head {
                    let repo = Repository::open(".").context("Not a git repository")?;
                    info!(
                        "Resetting branch to original commit before pull: {}",
                        orig_head
                    );
                    repo.reset(
                        &repo.find_object(
                            git2::Oid::from_str(orig_head)?,
                            Some(git2::ObjectType::Commit),
                        )?,
                        git2::ResetType::Hard,
                        None,
                    )?;
                }

                // Restore stashed changes if any
                if stash_created {
                    let mut repo = Repository::open(".").context("Not a git repository")?;
                    info!("Restoring stashed changes after abort");
                    // Only pop the stash created by sup (with message 'sup stash')
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
                            Ok(_) => info!("sup stash applied during abort"),
                            Err(e) => error!("Failed to apply sup stash during abort: {}", e),
                        }
                    } else {
                        info!("No sup stash found to apply during abort; likely already popped or not created");
                    }
                }
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
                ..
            } => {
                info!("Continuing interrupted operation");
                // 1. If a merge is in progress, finish it (assume user resolved conflicts and staged files)
                let mut repo = Repository::open(".").context("Not a git repository")?;
                if repo.state() == git2::RepositoryState::Merge {
                    info!("Finishing merge in progress (creating merge commit)");
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
                    info!("Merge commit created and merge state cleaned up");
                    // Ensure index is clean before applying stash
                    repo.reset(&repo.head()?.peel_to_commit()?.as_object(), git2::ResetType::Mixed, None)?;
                }
                // 2. Apply stash if it was created
                if stash_created {
                    info!("Applying stashed changes after merge");
                    // Use stash_apply and only drop if no conflicts
                    match repo.stash_apply(0, None) {
                        Ok(_) => {
                            info!("Stash applied");
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
                                };
                                state.save()?;
                                anyhow::bail!("Conflicts detected after stash apply");
                            } else {
                                info!("Dropping stash entry after successful apply");
                                repo.stash_drop(0)?;
                            }
                        }
                        Err(e) => {
                            error!("Failed to apply stash: {}", e);
                            state = SupState::Interrupted {
                                stash_created,
                                original_head: original_head.clone(),
                            };
                            state.save()?;
                            anyhow::bail!("Failed to apply stash");
                        }
                    }
                }
                SupState::clear()?;
                info!("sup --continue completed successfully");
                return Ok(());
            }
            _ => {
                anyhow::bail!("No interrupted operation to continue");
            }
        }
    }
    match state {
        SupState::InProgress { .. } => {
            anyhow::bail!("Operation already in progress. To roll back, run with --abort. To continue, run with --continue.");
        }
        _ => {}
    }
    let mut repo = Repository::open(".").context("Not a git repository")?;
    info!("Stashing changes (including untracked)");
    let sig = repo.signature()?;
    let stash_result = repo.stash_save(&sig, "sup stash", Some(StashFlags::INCLUDE_UNTRACKED));
    let stash_created = match stash_result {
        Ok(_) => true,
        Err(ref e) if e.code() == ErrorCode::NotFound => {
            info!("No changes to stash");
            false
        }
        Err(e) => return Err(e.into()),
    };
    let original_head = Some(repo.head()?.target().map(|oid| oid.to_string())).flatten();
    info!("Pulling latest changes from remote");
    if std::env::var("PULL_WITH_CLI").is_ok() {
        let status = std::process::Command::new("git").arg("pull").status()?;
        if !status.success() {
            error!("git pull failed");
            state = SupState::Interrupted {
                stash_created,
                original_head,
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
        if let Err(e) = crate::pull::pull_run(&args) {
            error!("git pull failed: {}", e);
            state = SupState::Interrupted {
                stash_created,
                original_head,
            };
            state.save()?;
            anyhow::bail!("git pull failed: {e}");
        }
    }
    state = SupState::InProgress {
        stash_created,
        original_head: original_head.clone(),
    };
    state.save()?;

    if stash_created {
        info!("Resetting index to HEAD before applying stashed changes");
        repo.reset(
            &repo.head()?.peel_to_commit()?.as_object(),
            git2::ResetType::Mixed,
            None,
        )?;
        info!("Applying stashed changes");
        match repo.stash_apply(0, None) {
            Ok(_) => {
                info!("Stash applied");
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
                    };
                    state.save()?;
                    anyhow::bail!("Conflicts detected after stash apply");
                } else {
                    info!("Stash applied successfully with no conflicts");
                }
            }
            Err(e) => {
                error!("Failed to apply stash: {}", e);
                state = SupState::Interrupted {
                    stash_created,
                    original_head,
                };
                state.save()?;
                anyhow::bail!("Failed to apply stash");
            }
        }
        info!("Dropping stash entry");
        repo.stash_drop(0)?;
    }
    info!("Operation completed successfully");
    SupState::clear()?;
    Ok(())
}
