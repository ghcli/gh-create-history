use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use git2::{Oid, Repository};

use crate::content::ContentGenerator;
use crate::git_ops::{self, TreeOp};

// ── helpers ──────────────────────────────────────────────────────────────────

fn get_branch_head(repo: &Repository, branch_name: &str) -> Result<Oid> {
    let branch = repo
        .find_branch(branch_name, git2::BranchType::Local)
        .with_context(|| format!("branch not found: {branch_name}"))?;
    branch
        .get()
        .target()
        .ok_or_else(|| anyhow::anyhow!("branch {branch_name} has no target"))
}

// ── public API ───────────────────────────────────────────────────────────────

/// Create a standard 2-parent merge commit and update the target branch.
pub fn create_merge_commit(
    repo: &Repository,
    source_branch: &str,
    target_branch: &str,
    time: DateTime<Utc>,
    _content_gen: &mut ContentGenerator,
    _max_size: u64,
) -> Result<Oid> {
    let source_oid = get_branch_head(repo, source_branch)?;
    let target_oid = get_branch_head(repo, target_branch)?;

    let target_commit = repo.find_commit(target_oid)?;
    let tree_oid = target_commit.tree_id();

    let message = format!("Merge branch '{source_branch}' into {target_branch}");
    let commit_oid =
        git_ops::create_commit(repo, tree_oid, &[target_oid, source_oid], &message, time)?;

    git_ops::update_branch(repo, target_branch, commit_oid)?;
    Ok(commit_oid)
}

/// Create an N-parent (octopus) merge commit and update the target branch.
pub fn create_octopus_merge(
    repo: &Repository,
    source_branches: &[String],
    target_branch: &str,
    time: DateTime<Utc>,
    _content_gen: &mut ContentGenerator,
    _max_size: u64,
) -> Result<Oid> {
    let target_oid = get_branch_head(repo, target_branch)?;
    let target_commit = repo.find_commit(target_oid)?;
    let tree_oid = target_commit.tree_id();

    let mut parents = vec![target_oid];
    for branch in source_branches {
        parents.push(get_branch_head(repo, branch)?);
    }

    let names: Vec<&str> = source_branches.iter().map(|s| s.as_str()).collect();
    let message = format!("Merge branches {} into {target_branch}", names.join(", "));

    let commit_oid = git_ops::create_commit(repo, tree_oid, &parents, &message, time)?;
    git_ops::update_branch(repo, target_branch, commit_oid)?;
    Ok(commit_oid)
}

/// Simulate a merge conflict: add a resolution file to the tree and record the
/// conflict in the commit message.
pub fn create_conflict_merge(
    repo: &Repository,
    source_branch: &str,
    target_branch: &str,
    time: DateTime<Utc>,
    content_gen: &mut ContentGenerator,
    max_size: u64,
) -> Result<Oid> {
    let source_oid = get_branch_head(repo, source_branch)?;
    let target_oid = get_branch_head(repo, target_branch)?;

    let target_commit = repo.find_commit(target_oid)?;
    let base_tree = target_commit.tree_id();

    let conflict_path = content_gen.generate_file_path();
    let resolution_content = content_gen.generate_file_content(max_size);

    let ops = vec![TreeOp::Add {
        path: conflict_path.clone(),
        content: resolution_content,
    }];
    let new_tree = git_ops::mutate_tree(repo, base_tree, &ops)?;

    let message = format!("Resolve merge conflict in {conflict_path}");
    let commit_oid =
        git_ops::create_commit(repo, new_tree, &[target_oid, source_oid], &message, time)?;

    git_ops::update_branch(repo, target_branch, commit_oid)?;
    Ok(commit_oid)
}

// ── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Create a repo with two diverged branches (`main` and `feature`).
    fn setup_two_branches() -> (TempDir, Repository) {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let time = Utc::now();

        let main_files = vec![("README.md".to_string(), b"# main".to_vec())];
        let main_tree = git_ops::build_initial_tree(&repo, &main_files).unwrap();
        let main_oid =
            git_ops::create_commit(&repo, main_tree, &[], "init main", time).unwrap();
        git_ops::update_branch(&repo, "main", main_oid).unwrap();

        let feat_tree = git_ops::mutate_tree(
            &repo,
            main_tree,
            &[TreeOp::Add {
                path: "feature.txt".to_string(),
                content: b"feat".to_vec(),
            }],
        )
        .unwrap();
        let feat_oid =
            git_ops::create_commit(&repo, feat_tree, &[main_oid], "feat work", time).unwrap();
        git_ops::update_branch(&repo, "feature", feat_oid).unwrap();

        (dir, repo)
    }

    /// Create a repo with three branches for octopus merge testing.
    fn setup_three_branches() -> (TempDir, Repository) {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let time = Utc::now();

        let files = vec![("base.txt".to_string(), b"base".to_vec())];
        let base_tree = git_ops::build_initial_tree(&repo, &files).unwrap();
        let base_oid =
            git_ops::create_commit(&repo, base_tree, &[], "init", time).unwrap();
        git_ops::update_branch(&repo, "main", base_oid).unwrap();

        for (name, file) in [("alpha", "alpha.txt"), ("beta", "beta.txt")] {
            let tree = git_ops::mutate_tree(
                &repo,
                base_tree,
                &[TreeOp::Add {
                    path: file.to_string(),
                    content: name.as_bytes().to_vec(),
                }],
            )
            .unwrap();
            let oid =
                git_ops::create_commit(&repo, tree, &[base_oid], &format!("{name} work"), time)
                    .unwrap();
            git_ops::update_branch(&repo, name, oid).unwrap();
        }

        (dir, repo)
    }

    fn make_content_gen() -> ContentGenerator {
        ContentGenerator::new(Some(42))
    }

    #[test]
    fn regular_merge() {
        let (_dir, repo) = setup_two_branches();
        let mut cg = make_content_gen();
        let time = Utc::now();

        let oid =
            create_merge_commit(&repo, "feature", "main", time, &mut cg, 1024).unwrap();
        let commit = repo.find_commit(oid).unwrap();
        assert_eq!(commit.parent_count(), 2);
        assert!(commit.message().unwrap().contains("Merge branch"));
    }

    #[test]
    fn octopus_merge() {
        let (_dir, repo) = setup_three_branches();
        let mut cg = make_content_gen();
        let time = Utc::now();

        let sources = vec!["alpha".to_string(), "beta".to_string()];
        let oid =
            create_octopus_merge(&repo, &sources, "main", time, &mut cg, 1024).unwrap();
        let commit = repo.find_commit(oid).unwrap();
        assert_eq!(commit.parent_count(), 3);
        assert!(commit.message().unwrap().contains("Merge branches"));
    }

    #[test]
    fn conflict_merge() {
        let (_dir, repo) = setup_two_branches();
        let mut cg = make_content_gen();
        let time = Utc::now();

        let oid =
            create_conflict_merge(&repo, "feature", "main", time, &mut cg, 1024).unwrap();
        let commit = repo.find_commit(oid).unwrap();
        assert_eq!(commit.parent_count(), 2);
        assert!(commit
            .message()
            .unwrap()
            .contains("Resolve merge conflict"));
    }
}
