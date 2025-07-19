fn file_url(path: &Path) -> String {
    let mut p = path.canonicalize().unwrap().to_string_lossy().replace("\\", "/");
    if p.starts_with("//?/" ) || p.starts_with("\\\\?\\") {
        p = p.trim_start_matches("//?/").trim_start_matches("\\\\?\\").to_string();
    }
    format!("file:///{}", p)
}

use std::process::Command;
use std::fs;
use std::path::Path;
use std::env;

fn run_cmd(dir: &Path, args: &[&str]) {
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
    println!("GIT STATUS BEFORE sup:\n{}", String::from_utf8_lossy(&git_status.stdout));

    if dir.join(".git/sup_state").exists() {
        println!("SUP STATE BEFORE sup:\n{}", file_content(&dir.join(".git/sup_state")));
    } else {
        println!("No SUP STATE found before sup.");
    }

    let exe = env!("CARGO_BIN_EXE_sup");
    let status = Command::new(exe)
        .args(extra_args)
        .current_dir(dir)
        .status()
        .expect("failed to run sup");
    
    // print git status after running sup
    let git_status = Command::new("git")
        .arg("status")
        .arg("-v")
        .current_dir(&dir)
        .output()
        .expect("failed to run git status");
    println!("GIT STATUS AFTER sup:\n{}", String::from_utf8_lossy(&git_status.stdout));
    if dir.join(".git/sup_state").exists() {
        println!("SUP STATE AFTER sup:\n{}", file_content(&dir.join(".git/sup_state")));
    } else {
        println!("No SUP STATE found after sup.");
    }

    if expect_failure {
        assert!(!status.success(), "sup should have failed, but it succeeded");
        return;
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
    run_cmd(&repo1, &["init"]);
    run_cmd(&repo1, &["config", "user.email", "test@example.com"]);
    run_cmd(&repo1, &["config", "user.name", "Test"]);
    fs::write(repo1.join("file.txt"), "initial\n").unwrap();
    run_cmd(&repo1, &["add", "."]);
    run_cmd(&repo1, &["commit", "-m", "initial"]);

    // clone repo1 to repo2
    let repo1_url = file_url(&repo1);
    run_cmd(&repo2, &["clone", &repo1_url, "."]);
    run_cmd(&repo2, &["config", "user.email", "test@example.com"]);
    run_cmd(&repo2, &["config", "user.name", "Test"]);

    // change repo1
    fs::write(repo1.join("file.txt"), "updated\n").unwrap();
    run_cmd(&repo1, &["add", "."]);
    run_cmd(&repo1, &["commit", "-m", "update"]);

    // run sup in repo2
    run_sup(&repo2, &[],false);

    // check repo2 file is updated
    let content = file_content(&repo2.join("file.txt"));
    assert_eq!(content, "updated\n");
}

#[test]
fn test_stash_and_pop_uncommitted_nonconflicting_changes() {
    let temp = tempfile::tempdir().unwrap();
    let repo1 = temp.path().join("repo1");
    let repo2 = temp.path().join("repo2");
    fs::create_dir(&repo1).unwrap();
    fs::create_dir(&repo2).unwrap();

    // init repo1
    run_cmd(&repo1, &["init"]);
    run_cmd(&repo1, &["config", "user.email", "test@example.com"]);
    run_cmd(&repo1, &["config", "user.name", "Test"]);
    fs::write(repo1.join("file.txt"), "initial\n").unwrap();
    run_cmd(&repo1, &["add", "."]);
    run_cmd(&repo1, &["commit", "-m", "initial"]);

    // clone repo1 to repo2
    let repo1_url = file_url(&repo1);
    run_cmd(&repo2, &["clone", &repo1_url, "."]);
    run_cmd(&repo2, &["config", "user.email", "test@example.com"]);
    run_cmd(&repo2, &["config", "user.name", "Test"]);

    // change repo1
    fs::write(repo1.join("file.txt"), "updated\n").unwrap();
    run_cmd(&repo1, &["add", "."]);
    run_cmd(&repo1, &["commit", "-m", "update"]);

    // make uncommitted change in repo2
    fs::write(repo2.join("file2.txt"), "localnewfile\n").unwrap();

    // run sup in repo2
    run_sup(&repo2, &[], false);

    // check repo2 file has local change (should be popped back)
    let content = file_content(&repo2.join("file2.txt"));
    assert_eq!(content, "localnewfile\n");

}

#[test]
fn test_stash_and_pop_uncommitted_conflicting_changes() {
    let temp = tempfile::tempdir().unwrap();
    let repo1 = temp.path().join("repo1");
    let repo2 = temp.path().join("repo2");
    fs::create_dir(&repo1).unwrap();
    fs::create_dir(&repo2).unwrap();

    // init repo1
    run_cmd(&repo1, &["init"]);
    run_cmd(&repo1, &["config", "user.email", "test@example.com"]);
    run_cmd(&repo1, &["config", "user.name", "Test"]);
    fs::write(repo1.join("file.txt"), "initial\n").unwrap();
    run_cmd(&repo1, &["add", "."]);
    run_cmd(&repo1, &["commit", "-m", "initial"]);

    // clone repo1 to repo2
    let repo1_url = file_url(&repo1);
    run_cmd(&repo2, &["clone", &repo1_url, "."]);
    run_cmd(&repo2, &["config", "user.email", "test@example.com"]);
    run_cmd(&repo2, &["config", "user.name", "Test"]);

    // change repo1
    fs::write(repo1.join("file.txt"), "updated\n").unwrap();
    run_cmd(&repo1, &["add", "."]);
    run_cmd(&repo1, &["commit", "-m", "update"]);

    // make uncommitted change in repo2
    fs::write(repo2.join("file1.txt"), "localchange\n").unwrap();

    // run sup in repo2
    run_sup(&repo2, &[], false);

    // check repo2 file has local change (should be popped back)
    let content = file_content(&repo2.join("file1.txt"));
    assert_eq!(content, "localchange\n");
}

#[test]
fn test_abort_on_conflicting_commit_and_uncommitted_change () {
    let temp = tempfile::tempdir().unwrap();
    let repo1 = temp.path().join("repo1");
    let repo2 = temp.path().join("repo2");
    fs::create_dir(&repo1).unwrap();
    fs::create_dir(&repo2).unwrap();

    // init repo1
    run_cmd(&repo1, &["init"]);
    run_cmd(&repo1, &["config", "user.email", "test@example.com"]);
    run_cmd(&repo1, &["config", "user.name", "Test"]);
    fs::write(repo1.join("file.txt"), "initial\n").unwrap();
    run_cmd(&repo1, &["add", "."]);
    run_cmd(&repo1, &["commit", "-m", "initial"]);

    // clone repo1 to repo2
    let repo1_url = file_url(&repo1);
    run_cmd(&repo2, &["clone", &repo1_url, "."]);
    run_cmd(&repo2, &["config", "user.email", "test@example.com"]);
    run_cmd(&repo2, &["config", "user.name", "Test"]);

    // change repo1
    fs::write(repo1.join("file.txt"), "updated\n").unwrap();
    run_cmd(&repo1, &["add", "."]);
    run_cmd(&repo1, &["commit", "-m", "update"]);

    // make commited conflicting change in repo2
    fs::write(repo2.join("file.txt"), "localchange\n").unwrap();
    run_cmd(&repo2, &["add", "."]);
    run_cmd(&repo2, &["commit", "-m", "local change"]);

    // make uncommitted non conflicting change in repo2
    fs::write(repo2.join("file2.txt"), "localnewfile\n").unwrap();

    // run sup in repo2
    run_sup(&repo2, &[], true);

    // show conflicting changes in file.txt
    let content = file_content(&repo2.join("file.txt"));
    assert_eq!(content, "<<<<<<< ours\nlocalchange\n=======\nupdated\n>>>>>>> theirs\n");

    // run sup with abort flag
    run_sup(&repo2, &["--abort"], false);

    // check repo2 file has both local changes (should be returned from pull abort and popped back)
    let content = file_content(&repo2.join("file.txt"));
    assert_eq!(content, "localchange\n");
    let content = file_content(&repo2.join("file2.txt"));
    assert_eq!(content, "localnewfile\n");
}

#[test]
fn test_abort_on_conflicting_uncommited_change() {
    let temp = tempfile::tempdir().unwrap();
    let repo1 = temp.path().join("repo1");
    let repo2 = temp.path().join("repo2");
    fs::create_dir(&repo1).unwrap();
    fs::create_dir(&repo2).unwrap();

    // init repo1
    run_cmd(&repo1, &["init"]);
    run_cmd(&repo1, &["config", "user.email", "test@example.com"]);
    run_cmd(&repo1, &["config", "user.name", "Test"]);
    fs::write(repo1.join("file.txt"), "initial\n").unwrap();
    run_cmd(&repo1, &["add", "."]);

    run_cmd(&repo1, &["commit", "-m", "initial"]);
    // clone repo1 to repo2
    let repo1_url = file_url(&repo1);
    run_cmd(&repo2, &["clone", &repo1_url, "."]);
    run_cmd(&repo2, &["config", "user.email", "test@example.com"]);
    run_cmd(&repo2, &["config", "user.name", "Test"]);
    // change repo1
    fs::write(repo1.join("file.txt"), "updated\n").unwrap();   
    run_cmd(&repo1, &["add", "."]);
    run_cmd(&repo1, &["commit", "-m", "update"]);
    // make uncommitted conflicting change in repo2
    fs::write(repo2.join("file.txt"), "localchange\n").unwrap();
    // run sup in repo2
    run_sup(&repo2, &[], true);
    // show conflicting changes in file.txt
    let content = file_content(&repo2.join("file.txt"));
    assert_eq!(content, "<<<<<<< Updated upstream\nupdated\n=======\nlocalchange\n>>>>>>> Stashed changes\n");
    // run sup with abort flag
    run_sup(&repo2, &["--abort"], false);

    // check repo2 file has local change (should be returned from abort)
    let content = file_content(&repo2.join("file.txt"));
    assert_eq!(content, "localchange\n");
}

#[test]
fn test_continue_applies_stash_after_conflict_resolution() {
    let temp = tempfile::tempdir().unwrap();
    let repo1 = temp.path().join("repo1");
    let repo2 = temp.path().join("repo2");
    fs::create_dir(&repo1).unwrap();
    fs::create_dir(&repo2).unwrap();

    // init repo1
    run_cmd(&repo1, &["init"]);
    run_cmd(&repo1, &["config", "user.email", "test@example.com"]);
    run_cmd(&repo1, &["config", "user.name", "Test"]);
    fs::write(repo1.join("file.txt"), "initial\n").unwrap();
    run_cmd(&repo1, &["add", "."]);
    run_cmd(&repo1, &["commit", "-m", "initial"]);

    // clone repo1 to repo2
    let repo1_url = file_url(&repo1);
    run_cmd(&repo2, &["clone", &repo1_url, "."]);
    run_cmd(&repo2, &["config", "user.email", "test@example.com"]);
    run_cmd(&repo2, &["config", "user.name", "Test"]);

    // change repo1 (remote)
    fs::write(repo1.join("file.txt"), "updated\n").unwrap();
    run_cmd(&repo1, &["add", "."]);
    run_cmd(&repo1, &["commit", "-m", "update"]);

    // make committed conflicting change in repo2
    fs::write(repo2.join("file.txt"), "localchange\n").unwrap();
    run_cmd(&repo2, &["add", "."]);
    run_cmd(&repo2, &["commit", "-m", "local change"]);

    // make uncommitted non-conflicting change in repo2
    fs::write(repo2.join("file2.txt"), "localnewfile\n").unwrap();

    // run sup in repo2 (should fail due to conflict)
    run_sup(&repo2, &[], true);

    // file.txt should have conflict markers
    let content = file_content(&repo2.join("file.txt"));
    assert!(content.contains("<<<<<<<") && content.contains("=======") && content.contains(">>>>>>>"));

    // resolve conflict manually (simulate user fix)
    fs::write(repo2.join("file.txt"), "resolved\n").unwrap();
    run_cmd(&repo2, &["add", "file.txt"]);
    // commit the resolution
    run_cmd(&repo2, &["commit", "-m", "resolve conflict"]);

    // run sup with --continue (should apply stashed changes)
    run_sup(&repo2, &["--continue"], false);

    // check that file2.txt (popped from stash) is present and correct
    let content = file_content(&repo2.join("file2.txt"));
    assert_eq!(content, "localnewfile\n");
    // check that file.txt has resolved content
    let content = file_content(&repo2.join("file.txt"));
    assert_eq!(content, "resolved\n");
}
