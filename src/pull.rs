/*
 * libgit2 "pull" example - shows how to pull remote data into a local branch.
 *
 * Written by the libgit2 contributors
 *
 * To the extent possible under law, the author(s) have dedicated all copyright
 * and related and neighboring rights to this software to the public domain
 * worldwide. This software is distributed without any warranty.
 *
 * You should have received a copy of the CC0 Public Domain Dedication along
 * with this software. If not, see
 * <http://creativecommons.org/publicdomain/zero/1.0/>.
 */

use git2::Repository;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::{str, time::Duration};
use structopt::StructOpt;

#[derive(StructOpt)]
pub(crate) struct Args {
    pub(crate) arg_remote: Option<String>,
    pub(crate) arg_branch: Option<String>,
}

pub(crate) struct Pulling {
    pub(crate) multi_progress: MultiProgress,
}

impl Pulling {
    fn do_fetch<'a>(
        &mut self,
        repo: &'a git2::Repository,
        refs: &[&str],
        remote: &'a mut git2::Remote,
        remote_tracking_ref: &str,
    ) -> Result<git2::AnnotatedCommit<'a>, git2::Error> {
        let mut cb = git2::RemoteCallbacks::new();

        let m = &self.multi_progress;

        let progress_style =
            ProgressStyle::with_template("[{elapsed}] {wide_bar:.cyan/blue} {pos:>7}/{len:7} {msg}")
                .unwrap()
                .progress_chars("=>-");
        let objects_progress = m.add(ProgressBar::no_length());
        objects_progress.set_style(progress_style.clone());
        objects_progress.set_message("Receiving objects  ");

        let deltas_progress = m.add(ProgressBar::no_length());
        let not_active_style = ProgressStyle::default_bar()
            .template("{msg}")
            .unwrap()
            .progress_chars("  ");
        deltas_progress.set_style(not_active_style);
        deltas_progress.set_message("Waiting to resolve deltas");
        
        let mut overflow_already_logged = false;

        cb.transfer_progress(move |stats| {
            if objects_progress.length().is_none() {
                objects_progress.set_length(stats.total_objects().try_into().unwrap_or(0));
            }
            if deltas_progress.length().is_none() && stats.total_deltas() > 0 {
                deltas_progress.set_message("Resolving deltas  ");
                deltas_progress.set_style(progress_style.clone());
                deltas_progress.set_length(stats.total_deltas().try_into().unwrap_or(0));
            }
            if stats.received_objects() == stats.total_objects() && !objects_progress.is_finished()
            {
                let bytes: u64 = stats.received_bytes().try_into().unwrap_or(u64::MAX);
                objects_progress.set_style(
                    ProgressStyle::default_bar()
                        .template(&format!(
                            "Received {} objects at {} ({})",
                            stats.received_objects(),
                            format!("{}s", objects_progress.elapsed().as_secs()),
                            indicatif::HumanBytes(bytes)
                        ))
                        .unwrap()
                        .progress_chars("=>-"),
                );
                objects_progress.finish_with_message("");
            }

            match stats.received_bytes().try_into() {
                Ok(received_bytes) => {
                    objects_progress.set_message(format!(
                        "Receiving objects ({})  ",
                        indicatif::HumanBytes(received_bytes)
                    ));
                }
                Err(_) => {
                    if !overflow_already_logged {
                        tracing::warn!("Received objects bytes overflowed");
                        overflow_already_logged = true;
                    }
                }
            }

            if stats.indexed_deltas() == stats.total_deltas() && stats.total_deltas() > 0 {
                deltas_progress.set_style(
                    ProgressStyle::default_bar()
                        .template(&format!(
                            "Resolved {} deltas at {}",
                            stats.indexed_deltas(),
                            format!("{}s", deltas_progress.elapsed().as_secs())
                        ))
                        .unwrap()
                        .progress_chars("=>-"),
                );
                deltas_progress.finish_with_message("");
            }
            objects_progress.set_position(stats.received_objects().try_into().unwrap_or(0));
            deltas_progress.set_position(stats.indexed_deltas().try_into().unwrap_or(0));

            true
        });

        cb.credentials(|url, username_from_url, allowed_types| {
            crate::credentials::callback(url, username_from_url, &allowed_types, repo)
        });

        let mut fo = git2::FetchOptions::new();
        fo.remote_callbacks(cb);
        // Always fetch all tags.
        // Perform a download and also update tips
        fo.download_tags(git2::AutotagOption::All);
        tracing::debug!("Fetching {} for repo", remote.name().unwrap());
        remote.fetch(refs, Some(&mut fo), None)?;

        // If there are local objects (we got a thin pack), then tell the user
        // how many objects we saved from having to cross the network.
        let stats = remote.stats();
        if stats.local_objects() > 0 {
            tracing::debug!(
                "Received {}/{} objects in {} bytes (used {} local objects)",
                stats.indexed_objects(),
                stats.total_objects(),
                stats.received_bytes(),
                stats.local_objects()
            );
        } else {
            tracing::debug!(
                "Received {}/{} objects in {} bytes",
                stats.indexed_objects(),
                stats.total_objects(),
                stats.received_bytes()
            );
        }

        // After fetch, return the AnnotatedCommit for the remote-tracking branch
        let fetch_ref = repo.find_reference(remote_tracking_ref)?;
        repo.reference_to_annotated_commit(&fetch_ref)
    }

    fn fast_forward(
        &mut self,
        repo: &Repository,
        lb: &mut git2::Reference,
        rc: &git2::AnnotatedCommit,
    ) -> Result<(), git2::Error> {
        let name = match lb.name() {
            Some(s) => s.to_string(),
            None => String::from_utf8_lossy(lb.name_bytes()).to_string(),
        };
        let msg = format!("Fast-Forward: Setting {} to id: {}", name, rc.id());
        tracing::debug!("{}", msg);
        lb.set_target(rc.id(), &msg)?;
        repo.set_head(&name)?;
        repo.checkout_head(Some(
            git2::build::CheckoutBuilder::default()
                // For some reason the force is required to make the working directory actually get updated
                // I suspect we should be adding some logic to handle dirty working directory states
                // but this is just an example so maybe not.
                .force(),
        ))?;
        Ok(())
    }

    fn normal_merge(
        &mut self,
        repo: &Repository,
        local: &git2::AnnotatedCommit,
        remote: &git2::AnnotatedCommit,
    ) -> Result<(), git2::Error> {
        let local_tree = repo.find_commit(local.id())?.tree()?;
        let remote_tree = repo.find_commit(remote.id())?.tree()?;
        let ancestor = repo
            .find_commit(repo.merge_base(local.id(), remote.id())?)?
            .tree()?;
        let mut idx = repo.merge_trees(&ancestor, &local_tree, &remote_tree, None)?;

        if idx.has_conflicts() {
            tracing::debug!("Merge conflicts detected...");
            repo.checkout_index(Some(&mut idx), None)?;
            // Set up merge state files so that the next git commit will be a merge commit
            use std::fs::File;
            use std::io::Write;
            // .git/MERGE_HEAD: remote commit id
            let git_dir = repo.path();
            let merge_head_path = git_dir.join("MERGE_HEAD");
            let mut merge_head = File::create(&merge_head_path)
                .map_err(|e| git2::Error::from_str(&format!("Failed to create MERGE_HEAD: {e}")))?;
            writeln!(merge_head, "{}", remote.id())
                .map_err(|e| git2::Error::from_str(&format!("Failed to write MERGE_HEAD: {e}")))?;
            // .git/MERGE_MSG: default merge message
            let merge_msg_path = git_dir.join("MERGE_MSG");
            let mut merge_msg = File::create(&merge_msg_path)
                .map_err(|e| git2::Error::from_str(&format!("Failed to create MERGE_MSG: {e}")))?;
            writeln!(merge_msg, "Merge: {} into {}", remote.id(), local.id())
                .map_err(|e| git2::Error::from_str(&format!("Failed to write MERGE_MSG: {e}")))?;
            // .git/MERGE_MODE: empty file (default)
            let merge_mode_path = git_dir.join("MERGE_MODE");
            File::create(&merge_mode_path)
                .map_err(|e| git2::Error::from_str(&format!("Failed to create MERGE_MODE: {e}")))?;
            return Err(git2::Error::from_str(
                "Merge conflicts detected, please resolve them manually.",
            ));
        }
        let result_tree = repo.find_tree(idx.write_tree_to(repo)?)?;
        // now create the merge commit
        let msg = format!("Merge: {} into {}", remote.id(), local.id());
        let sig = repo.signature()?;
        let local_commit = repo.find_commit(local.id())?;
        let remote_commit = repo.find_commit(remote.id())?;
        // Do our merge commit and set current branch head to that commit.
        let _merge_commit = repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            &msg,
            &result_tree,
            &[&local_commit, &remote_commit],
        )?;
        // Set working tree to match head.
        repo.checkout_head(None)?;
        Ok(())
    }

    fn do_merge<'a>(
        &mut self,
        repo: &'a Repository,
        remote_branch: &str,
        fetch_commit: git2::AnnotatedCommit<'a>,
    ) -> Result<(), git2::Error> {
        // 1. do a merge analysis
        let analysis = repo.merge_analysis(&[&fetch_commit])?;

        tracing::debug!("Merge analysis: {:?}", analysis.0);
        // 2. Do the appopriate merge
        if analysis.0.is_fast_forward() {
            tracing::debug!("Doing a fast forward");
            // do a fast forward
            let refname = format!("refs/heads/{}", remote_branch);
            match repo.find_reference(&refname) {
                Ok(mut r) => {
                    self.fast_forward(repo, &mut r, &fetch_commit)?;
                }
                Err(_) => {
                    // The branch doesn't exist so just set the reference to the
                    // commit directly. Usually this is because you are pulling
                    // into an empty repository.
                    repo.reference(
                        &refname,
                        fetch_commit.id(),
                        true,
                        &format!("Setting {} to {}", remote_branch, fetch_commit.id()),
                    )?;
                    repo.set_head(&refname)?;
                    repo.checkout_head(Some(
                        git2::build::CheckoutBuilder::default()
                            .allow_conflicts(true)
                            .conflict_style_merge(true)
                            .force(),
                    ))?;
                }
            };
        } else if analysis.0.is_normal() {
            tracing::debug!("Doing a normal merge");
            // do a normal merge
            let head_commit = repo.reference_to_annotated_commit(&repo.head()?)?;
            self.normal_merge(repo, &head_commit, &fetch_commit)?;
        } else {
            tracing::debug!("Nothing to merge, continue");
        }
        Ok(())
    }

    pub(crate) fn pull_run(&mut self, args: &Args) -> Result<(), git2::Error> {
        let remote_name = args.arg_remote.as_ref().map(|s| &s[..]).unwrap_or("origin");
        let remote_branch = args.arg_branch.as_ref().map(|s| &s[..]).unwrap_or("master");
        tracing::debug!("Pulling from remote: {}/{}", remote_name, remote_branch);
        let repo = Repository::open(".")?;
        let mut remote = repo.find_remote(remote_name)?;

        // Build refspec: refs/heads/main:refs/remotes/origin/main
        let refspec = format!(
            "refs/heads/{}:refs/remotes/{}/{}",
            remote_branch, remote_name, remote_branch
        );
        let remote_refname = format!("refs/remotes/{}/{}", remote_name, remote_branch);
        let fetch_commit = self.do_fetch(&repo, &[&refspec], &mut remote, &remote_refname)?;

        let merge_progress = self.multi_progress.add(ProgressBar::new_spinner());
        merge_progress
            .set_style(ProgressStyle::with_template("{spinner:.green} {wide_msg}").unwrap());
        merge_progress.set_message(format!(
            "Merging {} into {}",
            remote_branch,
            repo.head()?.name().unwrap_or("HEAD")
        ));
        merge_progress.enable_steady_tick(Duration::from_millis(100));

        let res = self.do_merge(&repo, remote_branch, fetch_commit);
        merge_progress.finish_with_message(format!("Merged branch {}", remote_branch));
        res
    }
}
