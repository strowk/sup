fn file_url(path: &Path) -> String {
    let mut p = path
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .replace("\\", "/");
    if p.starts_with("//?/") || p.starts_with("\\\\?\\") {
        p = p
            .trim_start_matches("//?/")
            .trim_start_matches("\\\\?\\")
            .to_string();
    }
    // On Unix, always ensure exactly one leading slash after file://
    #[cfg(unix)]
    {
        p = p.trim_start_matches('/').to_string();
    }
    format!("file:///{}", p)
}

use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

fn git_log(path: &Path) -> String {
    let git_log = Command::new("git")
        .arg("log")
        .arg("--graph")
        .arg("--format=%f")
        .arg("--all")
        .current_dir(path)
        .output()
        .expect("failed to run git log");
    String::from_utf8(git_log.stdout).unwrap()
}

fn run_git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .expect("failed to run git command");
    assert!(status.success(), "git command failed: {:?}", args);
}

fn run_sup(dir: &Path, extra_args: &[&str], expect_failure: bool) {
    // print git status before running sup
    let git_status = Command::new("git")
        .arg("status")
        .arg("-v")
        .current_dir(dir)
        .output()
        .expect("failed to run git status");
    println!(
        "GIT STATUS BEFORE sup:\n{}",
        String::from_utf8_lossy(&git_status.stdout)
    );

    if dir.join(".git/sup_state").exists() {
        println!(
            "SUP STATE BEFORE sup:\n{}",
            file_content(&dir.join(".git/sup_state"))
        );
    } else {
        println!("No SUP STATE found before sup.");
    }

    let exe = env!("CARGO_BIN_EXE_sup");
    let status = Command::new(exe)
        .args(extra_args)
        .current_dir(dir)
        .env("RUST_LOG", "debug")
        .status()
        .expect("failed to run sup");

    // print git status after running sup
    let git_status = Command::new("git")
        .arg("status")
        .arg("-v")
        .current_dir(dir)
        .output()
        .expect("failed to run git status");
    println!(
        "GIT STATUS AFTER sup:\n{}",
        String::from_utf8_lossy(&git_status.stdout)
    );
    if dir.join(".git/sup_state").exists() {
        println!(
            "SUP STATE AFTER sup:\n{}",
            file_content(&dir.join(".git/sup_state"))
        );
    } else {
        println!("No SUP STATE found after sup.");
    }

    let git_log = Command::new("git")
        .arg("log")
        .arg("--graph")
        .arg("--oneline")
        .arg("--all")
        .current_dir(dir)
        .output()
        .expect("failed to run git log");
    println!(
        "GIT LOG AFTER sup:\n{}",
        String::from_utf8_lossy(&git_log.stdout)
    );

    if expect_failure {
        assert!(
            !status.success(),
            "sup should have failed, but it succeeded"
        );
    } else {
        assert!(status.success(), "sup failed");
    }
}

fn file_content(path: &Path) -> String {
    fs::read_to_string(path).expect("failed to read file")
}

#[test]
fn test_pull_updates_repo() {
    let temp = tempfile::tempdir().unwrap();
    let repo1 = temp.path().join("repo1");
    let repo2 = temp.path().join("repo2");
    fs::create_dir(&repo1).unwrap();
    fs::create_dir(&repo2).unwrap();

    // init repo1
    run_git(&repo1, &["init"]);
    run_git(&repo1, &["config", "user.email", "test@example.com"]);
    run_git(&repo1, &["config", "user.name", "Test"]);
    fs::write(repo1.join("file.txt"), "initial\n").unwrap();
    run_git(&repo1, &["add", "."]);
    run_git(&repo1, &["commit", "-m", "initial"]);

    // clone repo1 to repo2
    let repo1_url = file_url(&repo1);
    run_git(&repo2, &["clone", &repo1_url, "."]);
    run_git(&repo2, &["config", "user.email", "test@example.com"]);
    run_git(&repo2, &["config", "user.name", "Test"]);

    // change repo1
    fs::write(repo1.join("file.txt"), "updated\n").unwrap();
    run_git(&repo1, &["add", "."]);
    run_git(&repo1, &["commit", "-m", "update"]);

    // run sup in repo2
    run_sup(&repo2, &[], false);

    // check repo2 file is updated
    let content = file_content(&repo2.join("file.txt"));
    assert_eq!(content, "updated\n");

    insta::assert_snapshot!(
    git_log(&repo2),
    @r"
    * update
    * initial
    ");
}

#[test]
fn test_stash_and_pop_uncommitted_nonconflicting_changes() {
    let temp = tempfile::tempdir().unwrap();
    let repo1 = temp.path().join("repo1");
    let repo2 = temp.path().join("repo2");
    fs::create_dir(&repo1).unwrap();
    fs::create_dir(&repo2).unwrap();

    // init repo1
    run_git(&repo1, &["init"]);
    run_git(&repo1, &["config", "user.email", "test@example.com"]);
    run_git(&repo1, &["config", "user.name", "Test"]);
    fs::write(repo1.join("file.txt"), "initial\n").unwrap();
    run_git(&repo1, &["add", "."]);
    run_git(&repo1, &["commit", "-m", "initial"]);

    // clone repo1 to repo2
    let repo1_url = file_url(&repo1);
    run_git(&repo2, &["clone", &repo1_url, "."]);
    run_git(&repo2, &["config", "user.email", "test@example.com"]);
    run_git(&repo2, &["config", "user.name", "Test"]);

    // change repo1
    fs::write(repo1.join("file.txt"), "updated\n").unwrap();
    run_git(&repo1, &["add", "."]);
    run_git(&repo1, &["commit", "-m", "update"]);

    // make uncommitted change in repo2
    fs::write(repo2.join("file2.txt"), "localnewfile\n").unwrap();

    // run sup in repo2
    run_sup(&repo2, &[], false);

    // check repo2 file has local change (should be popped back)
    let content = file_content(&repo2.join("file2.txt"));
    assert_eq!(content, "localnewfile\n");

    insta::assert_snapshot!(
    git_log(&repo2),
    @r"
    * update
    * initial
    ");
}

#[test]
fn test_stash_and_pop_uncommitted_and_commited_nonconflicting_changes() {
    let temp = tempfile::tempdir().unwrap();
    let repo1 = temp.path().join("repo1");
    let repo2 = temp.path().join("repo2");
    fs::create_dir(&repo1).unwrap();
    fs::create_dir(&repo2).unwrap();

    // init repo1
    run_git(&repo1, &["init"]);
    run_git(&repo1, &["config", "user.email", "test@example.com"]);
    run_git(&repo1, &["config", "user.name", "Test"]);
    fs::write(repo1.join("file.txt"), "initial\n").unwrap();
    fs::write(repo1.join("another_file.txt"), "another_initial\n").unwrap();
    run_git(&repo1, &["add", "."]);
    run_git(&repo1, &["commit", "-m", "initial"]);

    // clone repo1 to repo2
    let repo1_url = file_url(&repo1);
    run_git(&repo2, &["clone", &repo1_url, "."]);
    run_git(&repo2, &["config", "user.email", "test@example.com"]);
    run_git(&repo2, &["config", "user.name", "Test"]);

    // change repo1
    fs::write(repo1.join("file.txt"), "updated\n").unwrap();
    run_git(&repo1, &["add", "."]);
    run_git(&repo1, &["commit", "-m", "update"]);

    // make uncommitted change in repo2
    fs::write(repo2.join("file2.txt"), "localnewfile\n").unwrap();

    // make committed change in repo2
    fs::write(repo2.join("another_file.txt"), "local_change\n").unwrap();
    run_git(&repo2, &["add", "another_file.txt"]);
    run_git(&repo2, &["commit", "-m", "local change"]);

    // run sup in repo2
    run_sup(&repo2, &[], false);

    // check repo2 file has local change (should be popped back)
    let content = file_content(&repo2.join("file2.txt"));
    assert_eq!(content, "localnewfile\n");

    // check repo2 another_file.txt has local change (should remain during merge)
    let content = file_content(&repo2.join("another_file.txt"));
    assert_eq!(content, "local_change\n");

    // check repo2 file.txt is updated
    let content = file_content(&repo2.join("file.txt"));
    assert_eq!(content, "updated\n");

    insta::with_settings!({filters => vec![
        (r"\b-[[:xdigit:]]{40}\b", "-[HASH]"),
    ]}, {
    insta::assert_snapshot!(
    git_log(&repo2),
    @r"
    *   Merge-[HASH]-into-[HASH]
    |\  
    | * update
    * | local-change
    |/  
    * initial
    ");
    });
}

#[test]
fn test_stash_and_pop_uncommitted_conflicting_changes() {
    let temp = tempfile::tempdir().unwrap();
    let repo1 = temp.path().join("repo1");
    let repo2 = temp.path().join("repo2");
    fs::create_dir(&repo1).unwrap();
    fs::create_dir(&repo2).unwrap();

    // init repo1
    run_git(&repo1, &["init"]);
    run_git(&repo1, &["config", "user.email", "test@example.com"]);
    run_git(&repo1, &["config", "user.name", "Test"]);
    fs::write(repo1.join("file.txt"), "initial\n").unwrap();
    run_git(&repo1, &["add", "."]);
    run_git(&repo1, &["commit", "-m", "initial"]);

    // clone repo1 to repo2
    let repo1_url = file_url(&repo1);
    run_git(&repo2, &["clone", &repo1_url, "."]);
    run_git(&repo2, &["config", "user.email", "test@example.com"]);
    run_git(&repo2, &["config", "user.name", "Test"]);

    // change repo1
    fs::write(repo1.join("file.txt"), "updated\n").unwrap();
    run_git(&repo1, &["add", "."]);
    run_git(&repo1, &["commit", "-m", "update"]);

    // make uncommitted change in repo2
    fs::write(repo2.join("file1.txt"), "localchange\n").unwrap();

    // run sup in repo2
    run_sup(&repo2, &[], false);

    // check repo2 file has local change (should be popped back)
    let content = file_content(&repo2.join("file1.txt"));
    assert_eq!(content, "localchange\n");

    insta::assert_snapshot!(
    git_log(&repo2),
    @r"
    * update
    * initial
    ");
}

#[test]
fn test_abort_on_conflicting_commit_and_uncommitted_change() {
    let temp = tempfile::tempdir().unwrap();
    let repo1 = temp.path().join("repo1");
    let repo2 = temp.path().join("repo2");
    fs::create_dir(&repo1).unwrap();
    fs::create_dir(&repo2).unwrap();

    // init repo1
    run_git(&repo1, &["init"]);
    run_git(&repo1, &["config", "user.email", "test@example.com"]);
    run_git(&repo1, &["config", "user.name", "Test"]);
    fs::write(repo1.join("file.txt"), "initial\n").unwrap();
    run_git(&repo1, &["add", "."]);
    run_git(&repo1, &["commit", "-m", "initial"]);

    // clone repo1 to repo2
    let repo1_url = file_url(&repo1);
    run_git(&repo2, &["clone", &repo1_url, "."]);
    run_git(&repo2, &["config", "user.email", "test@example.com"]);
    run_git(&repo2, &["config", "user.name", "Test"]);

    // change repo1
    fs::write(repo1.join("file.txt"), "updated\n").unwrap();
    run_git(&repo1, &["add", "."]);
    run_git(&repo1, &["commit", "-m", "update"]);

    // make commited conflicting change in repo2
    fs::write(repo2.join("file.txt"), "localchange\n").unwrap();
    run_git(&repo2, &["add", "."]);
    run_git(&repo2, &["commit", "-m", "local change"]);

    // make uncommitted non conflicting change in repo2
    fs::write(repo2.join("file2.txt"), "localnewfile\n").unwrap();

    // run sup in repo2
    run_sup(&repo2, &[], true);

    // show conflicting changes in file.txt
    let content = file_content(&repo2.join("file.txt"));
    assert_eq!(
        content,
        "<<<<<<< ours\nlocalchange\n=======\nupdated\n>>>>>>> theirs\n"
    );

    // run sup with abort flag
    run_sup(&repo2, &["--abort"], false);

    // check repo2 file has both local changes (should be returned from pull abort and popped back)
    let content = file_content(&repo2.join("file.txt"));
    assert_eq!(content, "localchange\n");
    let content = file_content(&repo2.join("file2.txt"));
    assert_eq!(content, "localnewfile\n");

    insta::assert_snapshot!(
    git_log(&repo2),
    @r"
    * local-change
    | * update
    |/  
    * initial
    ");
}

#[test]
fn test_abort_on_conflicting_uncommited_change() {
    let temp = tempfile::tempdir().unwrap();
    let repo1 = temp.path().join("repo1");
    let repo2 = temp.path().join("repo2");
    fs::create_dir(&repo1).unwrap();
    fs::create_dir(&repo2).unwrap();

    // init repo1
    run_git(&repo1, &["init"]);
    run_git(&repo1, &["config", "user.email", "test@example.com"]);
    run_git(&repo1, &["config", "user.name", "Test"]);
    fs::write(repo1.join("file.txt"), "initial\n").unwrap();
    run_git(&repo1, &["add", "."]);

    run_git(&repo1, &["commit", "-m", "initial"]);
    // clone repo1 to repo2
    let repo1_url = file_url(&repo1);
    run_git(&repo2, &["clone", &repo1_url, "."]);
    run_git(&repo2, &["config", "user.email", "test@example.com"]);
    run_git(&repo2, &["config", "user.name", "Test"]);
    // change repo1
    fs::write(repo1.join("file.txt"), "updated\n").unwrap();
    run_git(&repo1, &["add", "."]);
    run_git(&repo1, &["commit", "-m", "update"]);
    // make uncommitted conflicting change in repo2
    fs::write(repo2.join("file.txt"), "localchange\n").unwrap();
    // run sup in repo2
    run_sup(&repo2, &[], true);
    // show conflicting changes in file.txt
    let content = file_content(&repo2.join("file.txt"));
    assert_eq!(
        content,
        "<<<<<<< Updated upstream\nupdated\n=======\nlocalchange\n>>>>>>> Stashed changes\n"
    );
    // run sup with abort flag
    run_sup(&repo2, &["--abort"], false);

    // check repo2 file has local change (should be returned from abort)
    let content = file_content(&repo2.join("file.txt"));
    assert_eq!(content, "localchange\n");

    insta::assert_snapshot!(
    git_log(&repo2),
    @r"
    * update
    * initial
    ");
}

#[test]
fn test_continue_applies_stash_after_conflict_resolution() {
    let temp = tempfile::tempdir().unwrap();
    let repo1 = temp.path().join("repo1");
    let repo2 = temp.path().join("repo2");
    fs::create_dir(&repo1).unwrap();
    fs::create_dir(&repo2).unwrap();

    // init repo1
    run_git(&repo1, &["init"]);
    run_git(&repo1, &["config", "user.email", "test@example.com"]);
    run_git(&repo1, &["config", "user.name", "Test"]);
    fs::write(repo1.join("file.txt"), "initial\n").unwrap();
    run_git(&repo1, &["add", "."]);
    run_git(&repo1, &["commit", "-m", "initial"]);

    // clone repo1 to repo2
    let repo1_url = file_url(&repo1);
    run_git(&repo2, &["clone", &repo1_url, "."]);
    run_git(&repo2, &["config", "user.email", "test@example.com"]);
    run_git(&repo2, &["config", "user.name", "Test"]);

    // change repo1 (remote)
    fs::write(repo1.join("file.txt"), "updated\n").unwrap();
    run_git(&repo1, &["add", "."]);
    run_git(&repo1, &["commit", "-m", "update"]);

    // make committed conflicting change in repo2
    fs::write(repo2.join("file.txt"), "localchange\n").unwrap();
    run_git(&repo2, &["add", "."]);
    run_git(&repo2, &["commit", "-m", "local change"]);

    // make uncommitted non-conflicting change in repo2
    fs::write(repo2.join("file2.txt"), "localnewfile\n").unwrap();

    // run sup in repo2 (should fail due to conflict)
    run_sup(&repo2, &[], true);

    // file.txt should have conflict markers
    let content = file_content(&repo2.join("file.txt"));
    assert!(
        content.contains("<<<<<<<") && content.contains("=======") && content.contains(">>>>>>>")
    );

    // resolve conflict manually (simulate user fix)
    fs::write(repo2.join("file.txt"), "resolved\n").unwrap();
    run_git(&repo2, &["add", "file.txt"]);
    // commit the resolution
    run_git(&repo2, &["commit", "-m", "resolve conflict"]);

    // run sup with --continue (should apply stashed changes)
    run_sup(&repo2, &["--continue"], false);

    // check that file2.txt (popped from stash) is present and correct
    let content = file_content(&repo2.join("file2.txt"));
    assert_eq!(content, "localnewfile\n");
    // check that file.txt has resolved content
    let content = file_content(&repo2.join("file.txt"));
    assert_eq!(content, "resolved\n");

    insta::assert_snapshot!(
    git_log(&repo2),
    @r"
    *   resolve-conflict
    |\  
    | * update
    * | local-change
    |/  
    * initial
    ");
}

#[test]
fn test_continue_applies_stash_after_conflict_resolution_then_commit_is_pushed() {
    let temp = tempfile::tempdir().unwrap();
    let repo1 = temp.path().join("repo1_bare");
    let repo2 = temp.path().join("repo2");
    // Create bare repo1
    run_git(temp.path(), &["init", "--bare", "repo1_bare"]);

    // Clone repo1 to repo2 (creates working directory)
    let repo1_url = file_url(&repo1);
    run_git(temp.path(), &["clone", &repo1_url, "repo2"]);
    run_git(&repo2, &["config", "user.email", "test@example.com"]);
    run_git(&repo2, &["config", "user.name", "Test"]);

    // Initial commit in repo2, then push to bare repo1
    fs::write(repo2.join("file.txt"), "initial\n").unwrap();
    run_git(&repo2, &["add", "."]);
    run_git(&repo2, &["commit", "-m", "initial"]);
    run_git(&repo2, &["push", "origin", "master"]);

    // Simulate remote change: clone repo1 to temp remote_work, commit, push
    let remote_work = temp.path().join("remote_work");
    run_git(temp.path(), &["clone", &repo1_url, "remote_work"]);
    run_git(&remote_work, &["config", "user.email", "test@example.com"]);
    run_git(&remote_work, &["config", "user.name", "Test"]);
    fs::write(remote_work.join("file.txt"), "updated\n").unwrap();
    run_git(&remote_work, &["add", "."]);
    run_git(&remote_work, &["commit", "-m", "update"]);
    run_git(&remote_work, &["push", "origin", "master"]);

    // In repo2: make committed conflicting change
    fs::write(repo2.join("file.txt"), "localchange\n").unwrap();
    run_git(&repo2, &["add", "."]);
    run_git(&repo2, &["commit", "-m", "local change"]);

    // make uncommitted non-conflicting change in repo2
    fs::write(repo2.join("file2.txt"), "localnewfile\n").unwrap();

    // run sup in repo2 (should fail due to conflict, but store commit message)
    run_sup(&repo2, &["-m", "commit message"], true);

    // file.txt should have conflict markers
    let content = file_content(&repo2.join("file.txt"));
    assert_eq!(
        content,
        "<<<<<<< ours\nlocalchange\n=======\nupdated\n>>>>>>> theirs\n"
    );

    // resolve conflict manually (simulate user fix)
    fs::write(repo2.join("file.txt"), "resolved\n").unwrap();
    run_git(&repo2, &["add", "file.txt"]);

    // commit the resolution
    run_git(&repo2, &["commit", "-m", "resolve conflict"]);

    // run sup with --continue (should apply stashed changes)
    run_sup(&repo2, &["--continue"], false);

    // check that file2.txt (popped from stash) is present and correct
    let content = file_content(&repo2.join("file2.txt"));
    assert_eq!(content, "localnewfile\n");
    // check that file.txt has resolved content
    let content = file_content(&repo2.join("file.txt"));
    assert_eq!(content, "resolved\n");

    // Clone remote to a third repo to verify push
    let verify_repo = temp.path().join("verify");
    run_git(temp.path(), &["clone", &repo1_url, "verify"]);
    let content = fs::read_to_string(verify_repo.join("file.txt")).unwrap();
    assert_eq!(content, "resolved\n");
    let content = fs::read_to_string(verify_repo.join("file2.txt")).unwrap();
    assert_eq!(content, "localnewfile\n");

    insta::assert_snapshot!(
    git_log(&repo2),
    @r"
    * commit-message
    *   resolve-conflict
    |\  
    | * update
    * | local-change
    |/  
    * initial
    ");
}

#[test]
fn test_continue_after_resolving_conflicting_change_from_stash() {
    let temp = tempfile::tempdir().unwrap();
    let repo1 = temp.path().join("repo1_bare");
    let repo2 = temp.path().join("repo2");
    // Create bare repo1
    run_git(temp.path(), &["init", "--bare", "repo1_bare"]);

    // Clone repo1 to repo2 (creates working directory)
    let repo1_url = file_url(&repo1);
    run_git(temp.path(), &["clone", &repo1_url, "repo2"]);
    run_git(&repo2, &["config", "user.email", "test@example.com"]);
    run_git(&repo2, &["config", "user.name", "Test"]);

    // Initial commit in repo2, then push to bare repo1
    fs::write(repo2.join("file.txt"), "initial\n").unwrap();
    run_git(&repo2, &["add", "."]);
    run_git(&repo2, &["commit", "-m", "initial"]);
    run_git(&repo2, &["push", "origin", "master"]);

    // Simulate remote change: clone repo1 to temp remote_work, commit, push
    let remote_work = temp.path().join("remote_work");
    run_git(temp.path(), &["clone", &repo1_url, "remote_work"]);
    run_git(&remote_work, &["config", "user.email", "test@example.com"]);
    run_git(&remote_work, &["config", "user.name", "Test"]);
    fs::write(remote_work.join("file.txt"), "updated\n").unwrap();
    run_git(&remote_work, &["add", "."]);
    run_git(&remote_work, &["commit", "-m", "update"]);
    run_git(&remote_work, &["push", "origin", "master"]);

    // In repo2: make conflicting change
    fs::write(repo2.join("file.txt"), "localchange\n").unwrap();

    // run sup in repo2 (should fail in conflict after stash pop)
    run_sup(&repo2, &["-m", "commit message"], true);

    // show conflicting changes in file.txt
    let content = file_content(&repo2.join("file.txt"));
    assert_eq!(
        content,
        "<<<<<<< Updated upstream\nupdated\n=======\nlocalchange\n>>>>>>> Stashed changes\n"
    );

    // continuing without resolving conflicts should fail
    run_sup(&repo2, &["--continue"], true);

    // resolve conflict manually (simulate user fix)
    fs::write(repo2.join("file.txt"), "resolved\n").unwrap();

    // stage resolved changes
    run_git(&repo2, &["add", "file.txt"]);

    // run sup with --continue, which is expected to drop the stash and apply commit
    run_sup(&repo2, &["--continue", "-y"], false);

    // check that file.txt has resolved content
    let content = file_content(&repo2.join("file.txt"));
    assert_eq!(content, "resolved\n");

    insta::assert_snapshot!(
        git_log(&repo2),
        @r"
    * commit-message
    * update
    * initial
    ");
}

#[test]
fn test_stash_and_pop_uncommitted_change_then_commit_with_hook_and_fail_on_exit_code() {
    let temp = tempfile::tempdir().unwrap();
    let repo1 = temp.path().join("repo1_bare");
    let repo2 = temp.path().join("repo2");
    // Create bare repo1
    run_git(temp.path(), &["init", "--bare", "repo1_bare"]);

    // Clone repo1 to repo2 (creates working directory)
    let repo1_url = file_url(&repo1);
    run_git(temp.path(), &["clone", &repo1_url, "repo2"]);
    run_git(&repo2, &["config", "user.email", "test@example.com"]);
    run_git(&repo2, &["config", "user.name", "Test"]);

    // Initial commit in repo2, then push to bare repo1
    fs::write(repo2.join("file.txt"), "initial\n").unwrap();
    run_git(&repo2, &["add", "."]);
    run_git(&repo2, &["commit", "-m", "initial"]);
    run_git(&repo2, &["push", "origin", "master"]);

    // Simulate remote change: clone repo1 to temp remote_work, commit, push
    let remote_work = temp.path().join("remote_work");
    run_git(temp.path(), &["clone", &repo1_url, "remote_work"]);
    run_git(&remote_work, &["config", "user.email", "test@example.com"]);
    run_git(&remote_work, &["config", "user.name", "Test"]);
    fs::write(remote_work.join("file.txt"), "updated\n").unwrap();
    run_git(&remote_work, &["add", "."]);
    run_git(&remote_work, &["commit", "-m", "update"]);
    run_git(&remote_work, &["push", "origin", "master"]);

    // make uncommitted change in repo2
    fs::write(repo2.join("file1.txt"), "localnewfile\n").unwrap();

    // Add a pre-commit hook that fails
    let hooks_dir = repo2.join(".git/hooks");
    fs::create_dir_all(&hooks_dir).unwrap();
    #[cfg(windows)]
    let hook_path = hooks_dir.join("pre-commit.bat");
    #[cfg(not(windows))]
    let hook_path = hooks_dir.join("pre-commit");
    #[cfg(windows)]
    fs::write(
        &hook_path,
        r#"
exit 1
"#
        .to_string()
        .as_bytes(),
    )
    .unwrap();
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::write(&hook_path, b"#!/bin/sh\nexit 1\n").unwrap();
        let mut perms = fs::metadata(&hook_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&hook_path, perms).unwrap();
    }

    // run sup in repo2
    run_sup(&repo2, &["-m", "commit message"], true);
}

#[test]
fn test_stash_and_pop_uncommitted_change_then_commit_with_pre_push_hook_and_fail_on_exit_code() {
    let temp = tempfile::tempdir().unwrap();
    let repo1 = temp.path().join("repo1_bare");
    let repo2 = temp.path().join("repo2");
    // Create bare repo1
    run_git(temp.path(), &["init", "--bare", "repo1_bare"]);

    // Clone repo1 to repo2 (creates working directory)
    let repo1_url = file_url(&repo1);
    run_git(temp.path(), &["clone", &repo1_url, "repo2"]);
    run_git(&repo2, &["config", "user.email", "test@example.com"]);
    run_git(&repo2, &["config", "user.name", "Test"]);

    // Initial commit in repo2, then push to bare repo1
    fs::write(repo2.join("file.txt"), "initial\n").unwrap();
    run_git(&repo2, &["add", "."]);
    run_git(&repo2, &["commit", "-m", "initial"]);
    run_git(&repo2, &["push", "origin", "master"]);

    // Simulate remote change: clone repo1 to temp remote_work, commit, push
    let remote_work = temp.path().join("remote_work");
    run_git(temp.path(), &["clone", &repo1_url, "remote_work"]);
    run_git(&remote_work, &["config", "user.email", "test@example.com"]);
    run_git(&remote_work, &["config", "user.name", "Test"]);
    fs::write(remote_work.join("file.txt"), "updated\n").unwrap();
    run_git(&remote_work, &["add", "."]);
    run_git(&remote_work, &["commit", "-m", "update"]);
    run_git(&remote_work, &["push", "origin", "master"]);

    // make uncommitted change in repo2
    fs::write(repo2.join("file1.txt"), "localnewfile\n").unwrap();

    // Add a pre-push hook that fails
    let hooks_dir = repo2.join(".git/hooks");
    fs::create_dir_all(&hooks_dir).unwrap();
    #[cfg(windows)]
    let hook_path = hooks_dir.join("pre-push.bat");
    #[cfg(not(windows))]
    let hook_path = hooks_dir.join("pre-push");
    #[cfg(windows)]
    fs::write(&hook_path, b"exit 1\n").unwrap();
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::write(&hook_path, b"#!/bin/sh\nexit 1\n").unwrap();
        let mut perms = fs::metadata(&hook_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&hook_path, perms).unwrap();
    }

    // run sup in repo2, should fail due to pre-push hook
    run_sup(&repo2, &["-m", "commit message"], true);

    insta::with_settings!({filters => vec![
        (r"\b[[:xdigit:]]{7}-initial\b", "[HASH]-initial"),
    ]}, {
    insta::assert_snapshot!(
    git_log(&repo2),
    @r"
    * commit-message
    * update
    | *   On-master-sup-stash
    |/|\  
    | | * untracked-files-on-master-[HASH]-initial
    | * index-on-master-[HASH]-initial
    |/  
    * initial
    ");
    });
}

#[test]
fn test_stash_and_pop_uncommitted_nonconflicting_changes_then_commit_with_hook_in_different_dir() {
    let temp = tempfile::tempdir().unwrap();
    let repo1 = temp.path().join("repo1_bare");
    let repo2 = temp.path().join("repo2");
    // Create bare repo1
    run_git(temp.path(), &["init", "--bare", "repo1_bare"]);

    // Clone repo1 to repo2 (creates working directory)
    let repo1_url = file_url(&repo1);
    run_git(temp.path(), &["clone", &repo1_url, "repo2"]);
    run_git(&repo2, &["config", "user.email", "test@example.com"]);
    run_git(&repo2, &["config", "user.name", "Test"]);

    // Initial commit in repo2, then push to bare repo1
    fs::write(repo2.join("file.txt"), "initial\n").unwrap();
    run_git(&repo2, &["add", "."]);
    run_git(&repo2, &["commit", "-m", "initial"]);
    run_git(&repo2, &["push", "origin", "master"]);

    // Simulate remote change: clone repo1 to temp remote_work, commit, push
    let remote_work = temp.path().join("remote_work");
    run_git(temp.path(), &["clone", &repo1_url, "remote_work"]);
    run_git(&remote_work, &["config", "user.email", "test@example.com"]);
    run_git(&remote_work, &["config", "user.name", "Test"]);
    fs::write(remote_work.join("file.txt"), "updated\n").unwrap();
    run_git(&remote_work, &["add", "."]);
    run_git(&remote_work, &["commit", "-m", "update"]);
    run_git(&remote_work, &["push", "origin", "master"]);

    // make uncommitted change in repo2
    fs::write(repo2.join("file2.txt"), "localnewfile\n").unwrap();

    // Add a pre-commit hook that fails
    let hooks_dir = repo2.join(".githooks");
    run_git(&repo2, &["config", "core.hooksPath", ".githooks"]);
    fs::create_dir_all(&hooks_dir).unwrap();
    #[cfg(windows)]
    let hook_path = hooks_dir.join("pre-commit.bat");
    #[cfg(not(windows))]
    let hook_path = hooks_dir.join("pre-commit");
    #[cfg(windows)]
    fs::write(
        &hook_path,
        r#"
exit 1
"#
        .to_string()
        .as_bytes(),
    )
    .unwrap();
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::write(&hook_path, b"#!/bin/sh\nexit 1\n").unwrap();
        let mut perms = fs::metadata(&hook_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&hook_path, perms).unwrap();
    }

    // run sup in repo2
    run_sup(&repo2, &["-m", "commit message"], true);

    // check repo2 file has local change (should be popped back)
    let content = file_content(&repo2.join("file2.txt"));
    assert_eq!(content, "localnewfile\n");
}

#[test]
fn test_commit_and_push_after_automatic_normal_merge() {
    let temp = tempfile::tempdir().unwrap();
    let repo1 = temp.path().join("repo1_bare");
    let repo2 = temp.path().join("repo2");
    // Create bare repo1
    run_git(temp.path(), &["init", "--bare", "repo1_bare"]);

    // Clone repo1 to repo2 (creates working directory)
    let repo1_url = file_url(&repo1);
    run_git(temp.path(), &["clone", &repo1_url, "repo2"]);
    run_git(&repo2, &["config", "user.email", "test@example.com"]);
    run_git(&repo2, &["config", "user.name", "Test"]);

    // Initial commit in repo2, then push to bare repo1
    fs::write(
        repo2.join("file.txt"),
        "line0\ninitial-line1\nline2\nline3\nline4\nline5\n",
    )
    .unwrap();
    run_git(&repo2, &["add", "."]);
    run_git(&repo2, &["commit", "-m", "initial"]);
    run_git(&repo2, &["push", "origin", "master"]);

    // Simulate remote change: clone repo1 to temp remote_work, commit, push
    let remote_work = temp.path().join("remote_work");
    run_git(temp.path(), &["clone", &repo1_url, "remote_work"]);
    run_git(&remote_work, &["config", "user.email", "test@example.com"]);
    run_git(&remote_work, &["config", "user.name", "Test"]);
    fs::write(
        remote_work.join("file.txt"),
        "line0\nupdated-line1\nline2\nline3\nline4\nline5\n",
    )
    .unwrap();
    run_git(&remote_work, &["add", "."]);
    run_git(&remote_work, &["commit", "-m", "update"]);
    run_git(&remote_work, &["push", "origin", "master"]);

    // In repo2: make committed non-conflicting change that requires a normal merge
    fs::write(
        repo2.join("file.txt"),
        "line0\ninitial-line1\nline2\nline3\nline4-changed\nline5\n",
    )
    .unwrap();
    run_git(&repo2, &["add", "."]);
    run_git(&repo2, &["commit", "-m", "local change"]);

    // make uncommitted non-conflicting change in repo2
    fs::write(repo2.join("file2.txt"), "localnewfile\n").unwrap();

    // run sup in repo2 (should succeed and push changes)
    run_sup(&repo2, &["-m", "commit message"], false);

    // check that file2.txt (popped from stash) is present and correct
    let content = file_content(&repo2.join("file2.txt"));
    assert_eq!(content, "localnewfile\n");

    // check that file.txt has merged content
    let content = file_content(&repo2.join("file.txt"));
    assert_eq!(
        content,
        "line0\nupdated-line1\nline2\nline3\nline4-changed\nline5\n"
    );

    // verify that the commit was pushed to the remote
    let verify_repo = temp.path().join("verify");
    run_git(temp.path(), &["clone", &repo1_url, "verify"]);
    let content = fs::read_to_string(verify_repo.join("file.txt")).unwrap();
    assert_eq!(
        content,
        "line0\nupdated-line1\nline2\nline3\nline4-changed\nline5\n"
    );
    let content = fs::read_to_string(verify_repo.join("file2.txt")).unwrap();
    assert_eq!(content, "localnewfile\n");

    insta::with_settings!({filters => vec![
        (r"\b-[[:xdigit:]]{40}\b", "-[HASH]"),
    ]}, {
    insta::assert_snapshot!(
    git_log(&repo2),
    @r"
    * commit-message
    *   Merge-[HASH]-into-[HASH]
    |\  
    | * update
    * | local-change
    |/  
    * initial
    ");
    });
}
