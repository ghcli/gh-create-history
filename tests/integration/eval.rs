use gh_create_history::cli::Args;
use gh_create_history::engine;
use clap::Parser;
use tempfile::TempDir;

/// Run a git command in the given repo and return trimmed stdout.
fn git_cmd(repo_path: &std::path::Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .expect("failed to execute git");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Run a git command and count non-empty lines in stdout.
fn git_count(repo_path: &std::path::Path, args: &[&str]) -> usize {
    git_cmd(repo_path, args)
        .lines()
        .filter(|l| !l.is_empty())
        .count()
}

fn run_engine(args: Args) {
    engine::run(args).expect("engine::run failed");
}

// ---------------------------------------------------------------------------
// 1. Small repo — basic sanity
// ---------------------------------------------------------------------------
#[test]
fn test_small_repo() {
    let tmp = TempDir::new().unwrap();
    let args = Args::parse_from([
        "test",
        "--commits", "10",
        "--branches", "3",
        "--size", "512b",
        "--oldest", "30d",
        "--seed", "1",
        "--quiet",
        "--repo-path", tmp.path().to_str().unwrap(),
    ]);
    run_engine(args);

    // 3 feature branches + main = 4
    let branch_count = git_count(tmp.path(), &["branch", "--list"]);
    assert_eq!(branch_count, 4, "expected 4 branches (3 + main), got {branch_count}");

    // At least 30 commits across all branches (10 per branch × 3 branches + main)
    let commit_count = git_count(tmp.path(), &["rev-list", "--all"]);
    assert!(commit_count >= 30, "expected >= 30 commits, got {commit_count}");

    // Repository integrity
    let fsck = git_cmd(tmp.path(), &["fsck", "--no-progress"]);
    assert!(
        !fsck.contains("error") && !fsck.contains("fatal"),
        "git fsck reported errors:\n{fsck}"
    );
}

// ---------------------------------------------------------------------------
// 2. Medium repo — full feature matrix
// ---------------------------------------------------------------------------
#[test]
fn test_medium_repo() {
    let tmp = TempDir::new().unwrap();
    let args = Args::parse_from([
        "test",
        "--commits", "50",
        "--branches", "10",
        "--size", "1024b",
        "--oldest", "180d",
        "--seed", "42",
        "--quiet",
        "--repo-path", tmp.path().to_str().unwrap(),
    ]);
    run_engine(args);

    // Branches: 10 feature + main = 11
    let branch_count = git_count(tmp.path(), &["branch", "--list"]);
    assert_eq!(branch_count, 11, "expected 11 branches, got {branch_count}");

    // Commits
    let commit_count = git_count(tmp.path(), &["rev-list", "--all"]);
    assert!(commit_count >= 50, "expected >= 50 total commits, got {commit_count}");

    // Merge commits
    let merges = git_count(tmp.path(), &["rev-list", "--all", "--merges"]);
    assert!(merges > 0, "expected merge commits, got 0");

    // Octopus merges (3+ parents)
    let octopus_output = git_cmd(tmp.path(), &[
        "rev-list", "--all", "--min-parents=3",
    ]);
    let octopus = octopus_output.lines().filter(|l| !l.is_empty()).count();
    assert!(octopus > 0, "expected octopus merges, got 0");

    // Tags
    let tag_count = git_count(tmp.path(), &["tag", "--list"]);
    assert!(tag_count > 0, "expected tags, got 0");

    // Renames (look for rename entries in diff)
    let renames = git_cmd(tmp.path(), &[
        "log", "--all", "--diff-filter=R", "--summary", "--oneline",
    ]);
    assert!(!renames.is_empty(), "expected file renames in history");

    // Deletes
    let deletes = git_cmd(tmp.path(), &[
        "log", "--all", "--diff-filter=D", "--summary", "--oneline",
    ]);
    assert!(!deletes.is_empty(), "expected file deletes in history");

    // Max blob size <= 1024
    let blob_sizes = git_cmd(tmp.path(), &[
        "rev-list", "--all", "--objects",
    ]);
    // Check via cat-file that no blob exceeds the cap
    for line in blob_sizes.lines() {
        let sha = line.split_whitespace().next().unwrap_or("");
        if sha.is_empty() { continue; }
        let obj_type = git_cmd(tmp.path(), &["cat-file", "-t", sha]);
        if obj_type == "blob" {
            let size_str = git_cmd(tmp.path(), &["cat-file", "-s", sha]);
            let size: u64 = size_str.parse().unwrap_or(0);
            assert!(
                size <= 1024,
                "blob {sha} is {size} bytes, exceeds 1024 cap"
            );
        }
    }

    // fsck
    let fsck = git_cmd(tmp.path(), &["fsck", "--no-progress"]);
    assert!(
        !fsck.contains("error") && !fsck.contains("fatal"),
        "git fsck reported errors:\n{fsck}"
    );
}

// ---------------------------------------------------------------------------
// 3. Deterministic seed — identical repos from identical params
// ---------------------------------------------------------------------------
#[test]
fn test_deterministic_seed() {
    let tmp1 = TempDir::new().unwrap();
    let tmp2 = TempDir::new().unwrap();

    for tmp in [&tmp1, &tmp2] {
        let args = Args::parse_from([
            "test",
            "--commits", "15",
            "--branches", "3",
            "--size", "512b",
            "--oldest", "30d",
            "--seed", "9999",
            "--quiet",
            "--repo-path", tmp.path().to_str().unwrap(),
        ]);
        run_engine(args);
    }

    let head1 = git_cmd(tmp1.path(), &["rev-parse", "HEAD"]);
    let head2 = git_cmd(tmp2.path(), &["rev-parse", "HEAD"]);

    assert_eq!(
        head1, head2,
        "identical seed+params should produce identical HEAD SHAs"
    );
}

// ---------------------------------------------------------------------------
// 4. Existing repo — original commit remains reachable
// ---------------------------------------------------------------------------
#[test]
fn test_existing_repo() {
    let tmp = TempDir::new().unwrap();

    // Initialise a repo with one commit
    git_cmd(tmp.path(), &["init"]);
    git_cmd(tmp.path(), &["-c", "user.name=Test", "-c", "user.email=t@t.com",
        "commit", "--allow-empty", "-m", "seed commit"]);
    let original_sha = git_cmd(tmp.path(), &["rev-parse", "HEAD"]);

    let args = Args::parse_from([
        "test",
        "--commits", "10",
        "--branches", "2",
        "--size", "512b",
        "--oldest", "30d",
        "--seed", "77",
        "--quiet",
        "--repo-path", tmp.path().to_str().unwrap(),
    ]);
    run_engine(args);

    // Original commit must still be reachable
    let found = git_cmd(tmp.path(), &["rev-list", "--all"]);
    assert!(
        found.contains(&original_sha),
        "original commit {original_sha} is no longer reachable"
    );
}

// ---------------------------------------------------------------------------
// 5. Size cap — every blob within limit
// ---------------------------------------------------------------------------
#[test]
fn test_size_cap() {
    let tmp = TempDir::new().unwrap();
    let args = Args::parse_from([
        "test",
        "--commits", "20",
        "--branches", "5",
        "--size", "256b",
        "--oldest", "30d",
        "--seed", "256",
        "--quiet",
        "--repo-path", tmp.path().to_str().unwrap(),
    ]);
    run_engine(args);

    let objects = git_cmd(tmp.path(), &["rev-list", "--all", "--objects"]);
    for line in objects.lines() {
        let sha = line.split_whitespace().next().unwrap_or("");
        if sha.is_empty() { continue; }
        let obj_type = git_cmd(tmp.path(), &["cat-file", "-t", sha]);
        if obj_type == "blob" {
            let size_str = git_cmd(tmp.path(), &["cat-file", "-s", sha]);
            let size: u64 = size_str.parse().unwrap_or(0);
            assert!(
                size <= 256,
                "blob {sha} is {size} bytes, exceeds 256b cap"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 6. Time spread — commits cover at least 50% of the window
// ---------------------------------------------------------------------------
#[test]
fn test_time_spread() {
    let tmp = TempDir::new().unwrap();
    let window_days: i64 = 365;
    let args = Args::parse_from([
        "test",
        "--commits", "50",
        "--branches", "5",
        "--size", "512b",
        "--oldest", "365d",
        "--seed", "365",
        "--quiet",
        "--repo-path", tmp.path().to_str().unwrap(),
    ]);
    run_engine(args);

    // Get oldest and newest author timestamps
    let oldest_ts = git_cmd(tmp.path(), &[
        "log", "--all", "--format=%at", "--reverse",
    ]);
    let newest_ts = git_cmd(tmp.path(), &[
        "log", "--all", "--format=%at",
    ]);

    let oldest_epoch: i64 = oldest_ts
        .lines().next().unwrap_or("0")
        .parse().unwrap_or(0);
    let newest_epoch: i64 = newest_ts
        .lines().next().unwrap_or("0")
        .parse().unwrap_or(0);

    let actual_span_days = (newest_epoch - oldest_epoch) / 86400;
    let min_span = window_days / 2; // 50%

    assert!(
        actual_span_days >= min_span,
        "time spread {actual_span_days}d is less than 50% of {window_days}d window"
    );
}
