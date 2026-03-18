use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use git2::{IndexEntry, IndexTime, Oid, Repository, Signature, Time};

/// Operations that can be applied to a tree to produce a new tree.
pub enum TreeOp {
    Add { path: String, content: Vec<u8> },
    Modify { path: String, content: Vec<u8> },
    Rename { old_path: String, new_path: String },
    Delete { path: String },
}

/// Open an existing repository or initialise a new one.
pub fn init_or_open_repo(path: &Path) -> Result<Repository> {
    if path.join(".git").exists() {
        Repository::open(path).context("failed to open existing repository")
    } else {
        Repository::init(path).context("failed to initialise new repository")
    }
}

/// Write raw bytes into the object database and return the blob OID.
pub fn create_blob(repo: &Repository, content: &[u8]) -> Result<Oid> {
    repo.blob(content).context("failed to create blob")
}

/// Build a new tree from a flat list of `(path, content)` pairs.
/// Nested directories (e.g. `"src/lib.rs"`) are handled automatically via the
/// in-memory index.
pub fn build_initial_tree(repo: &Repository, files: &[(String, Vec<u8>)]) -> Result<Oid> {
    let mut index = git2::Index::new()?;

    for (path, content) in files {
        let blob_oid = repo.blob(content)?;
        let entry = make_index_entry(path, blob_oid, content.len() as u32);
        index.add(&entry)?;
    }

    let tree_oid = index.write_tree_to(repo)?;
    Ok(tree_oid)
}

/// Apply a sequence of [`TreeOp`]s to an existing tree and return the new tree
/// OID.  Handles nested paths correctly.
pub fn mutate_tree(repo: &Repository, base_tree: Oid, ops: &[TreeOp]) -> Result<Oid> {
    let tree = repo.find_tree(base_tree)?;
    let mut index = git2::Index::new()?;
    index.read_tree(&tree)?;

    for op in ops {
        match op {
            TreeOp::Add { path, content } | TreeOp::Modify { path, content } => {
                let blob_oid = repo.blob(content)?;
                let entry = make_index_entry(path, blob_oid, content.len() as u32);
                index.add(&entry)?;
            }
            TreeOp::Rename { old_path, new_path } => {
                let old_entry = index
                    .get_path(Path::new(old_path), 0)
                    .ok_or_else(|| anyhow::anyhow!("path not found for rename: {old_path}"))?;
                let blob_oid = old_entry.id;
                let size = old_entry.file_size;
                index.remove(Path::new(old_path), 0)?;
                let entry = make_index_entry(new_path, blob_oid, size);
                index.add(&entry)?;
            }
            TreeOp::Delete { path } => {
                index.remove(Path::new(path), 0)?;
            }
        }
    }

    let tree_oid = index.write_tree_to(repo)?;
    Ok(tree_oid)
}

/// Create a commit in the repository.
///
/// Author and committer are set to **"Load Test Bot \<loadtest@example.com\>"**
/// at the supplied `time`.  The commit is *not* attached to any ref — callers
/// should use [`update_branch`] afterwards.
pub fn create_commit(
    repo: &Repository,
    tree_oid: Oid,
    parents: &[Oid],
    message: &str,
    time: DateTime<Utc>,
) -> Result<Oid> {
    let git_time = datetime_to_git_time(time);
    let sig = Signature::new("Load Test Bot", "loadtest@example.com", &git_time)?;
    let tree = repo.find_tree(tree_oid)?;

    let parent_commits: Vec<git2::Commit> = parents
        .iter()
        .map(|oid| repo.find_commit(*oid))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let parent_refs: Vec<&git2::Commit> = parent_commits.iter().collect();

    let oid = repo.commit(None, &sig, &sig, message, &tree, &parent_refs)?;
    Ok(oid)
}

/// Point a local branch at the given commit (create or force-update).
/// Uses direct reference manipulation to avoid the libgit2 limitation where
/// `repo.branch()` cannot force-update the current HEAD branch.
pub fn update_branch(repo: &Repository, branch_name: &str, commit_oid: Oid) -> Result<()> {
    let refname = format!("refs/heads/{}", branch_name);
    match repo.find_reference(&refname) {
        Ok(mut r) => {
            r.set_target(commit_oid, "update branch")?;
        }
        Err(_) => {
            repo.reference(&refname, commit_oid, false, "create branch")?;
        }
    }
    Ok(())
}

/// Create a lightweight tag pointing at the given commit.
pub fn create_tag(
    repo: &Repository,
    tag_name: &str,
    commit_oid: Oid,
    _time: DateTime<Utc>,
) -> Result<()> {
    let obj = repo.find_object(commit_oid, Some(git2::ObjectType::Commit))?;
    repo.tag_lightweight(tag_name, &obj, true)?;
    Ok(())
}

/// Shell out to `git push --all && git push --tags`.
pub fn push_all(repo: &Repository) -> Result<()> {
    let workdir = repo
        .workdir()
        .or_else(|| repo.path().parent())
        .ok_or_else(|| anyhow::anyhow!("cannot determine repository working directory"))?;

    let status = Command::new("git")
        .args(["push", "--all"])
        .current_dir(workdir)
        .status()
        .context("failed to run git push --all")?;
    if !status.success() {
        anyhow::bail!("git push --all failed with status {status}");
    }

    let status = Command::new("git")
        .args(["push", "--tags"])
        .current_dir(workdir)
        .status()
        .context("failed to run git push --tags")?;
    if !status.success() {
        anyhow::bail!("git push --tags failed with status {status}");
    }

    Ok(())
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn datetime_to_git_time(dt: DateTime<Utc>) -> Time {
    Time::new(dt.timestamp(), 0)
}

fn make_index_entry(path: &str, blob_oid: Oid, size: u32) -> IndexEntry {
    IndexEntry {
        ctime: IndexTime::new(0, 0),
        mtime: IndexTime::new(0, 0),
        dev: 0,
        ino: 0,
        mode: 0o100644,
        uid: 0,
        gid: 0,
        file_size: size,
        id: blob_oid,
        flags: (path.len() as u16) & 0xFFF,
        flags_extended: 0,
        path: path.as_bytes().to_vec(),
    }
}

// ── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, Repository) {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        (dir, repo)
    }

    #[test]
    fn init_new_repo() {
        let dir = TempDir::new().unwrap();
        let repo = init_or_open_repo(dir.path()).unwrap();
        assert!(repo.path().exists());
    }

    #[test]
    fn open_existing_repo() {
        let dir = TempDir::new().unwrap();
        Repository::init(dir.path()).unwrap();
        let repo = init_or_open_repo(dir.path()).unwrap();
        assert!(repo.path().exists());
    }

    #[test]
    fn blob_round_trip() {
        let (_dir, repo) = setup();
        let oid = create_blob(&repo, b"hello world").unwrap();
        let blob = repo.find_blob(oid).unwrap();
        assert_eq!(blob.content(), b"hello world");
    }

    #[test]
    fn build_tree_flat_files() {
        let (_dir, repo) = setup();
        let files = vec![
            ("a.txt".to_string(), b"aaa".to_vec()),
            ("b.txt".to_string(), b"bbb".to_vec()),
        ];
        let tree_oid = build_initial_tree(&repo, &files).unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        assert!(tree.get_name("a.txt").is_some());
        assert!(tree.get_name("b.txt").is_some());
    }

    #[test]
    fn build_tree_nested_dirs() {
        let (_dir, repo) = setup();
        let files = vec![
            ("README.md".to_string(), b"# Hi".to_vec()),
            ("src/main.rs".to_string(), b"fn main() {}".to_vec()),
            ("src/lib.rs".to_string(), b"// lib".to_vec()),
        ];
        let tree_oid = build_initial_tree(&repo, &files).unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        assert!(tree.get_name("README.md").is_some());
        assert!(tree.get_name("src").is_some());
    }

    #[test]
    fn mutate_tree_add() {
        let (_dir, repo) = setup();
        let files = vec![("a.txt".to_string(), b"aaa".to_vec())];
        let tree_oid = build_initial_tree(&repo, &files).unwrap();

        let ops = vec![TreeOp::Add {
            path: "b.txt".to_string(),
            content: b"bbb".to_vec(),
        }];
        let new_oid = mutate_tree(&repo, tree_oid, &ops).unwrap();
        let tree = repo.find_tree(new_oid).unwrap();
        assert!(tree.get_name("a.txt").is_some());
        assert!(tree.get_name("b.txt").is_some());
    }

    #[test]
    fn mutate_tree_modify() {
        let (_dir, repo) = setup();
        let files = vec![("a.txt".to_string(), b"old".to_vec())];
        let tree_oid = build_initial_tree(&repo, &files).unwrap();

        let ops = vec![TreeOp::Modify {
            path: "a.txt".to_string(),
            content: b"new".to_vec(),
        }];
        let new_oid = mutate_tree(&repo, tree_oid, &ops).unwrap();
        assert_ne!(tree_oid, new_oid);

        let tree = repo.find_tree(new_oid).unwrap();
        let entry = tree.get_name("a.txt").unwrap();
        let blob = repo.find_blob(entry.id()).unwrap();
        assert_eq!(blob.content(), b"new");
    }

    #[test]
    fn mutate_tree_delete() {
        let (_dir, repo) = setup();
        let files = vec![
            ("a.txt".to_string(), b"aaa".to_vec()),
            ("b.txt".to_string(), b"bbb".to_vec()),
        ];
        let tree_oid = build_initial_tree(&repo, &files).unwrap();

        let ops = vec![TreeOp::Delete {
            path: "a.txt".to_string(),
        }];
        let new_oid = mutate_tree(&repo, tree_oid, &ops).unwrap();
        let tree = repo.find_tree(new_oid).unwrap();
        assert!(tree.get_name("a.txt").is_none());
        assert!(tree.get_name("b.txt").is_some());
    }

    #[test]
    fn mutate_tree_rename() {
        let (_dir, repo) = setup();
        let files = vec![("old.txt".to_string(), b"content".to_vec())];
        let tree_oid = build_initial_tree(&repo, &files).unwrap();

        let ops = vec![TreeOp::Rename {
            old_path: "old.txt".to_string(),
            new_path: "new.txt".to_string(),
        }];
        let new_oid = mutate_tree(&repo, tree_oid, &ops).unwrap();
        let tree = repo.find_tree(new_oid).unwrap();
        assert!(tree.get_name("old.txt").is_none());
        assert!(tree.get_name("new.txt").is_some());
    }

    #[test]
    fn create_root_commit() {
        let (_dir, repo) = setup();
        let files = vec![("file.txt".to_string(), b"data".to_vec())];
        let tree_oid = build_initial_tree(&repo, &files).unwrap();
        let time = Utc::now();

        let oid = create_commit(&repo, tree_oid, &[], "initial commit", time).unwrap();
        let commit = repo.find_commit(oid).unwrap();
        assert_eq!(commit.message(), Some("initial commit"));
        assert_eq!(commit.author().name(), Some("Load Test Bot"));
        assert_eq!(commit.parent_count(), 0);
    }

    #[test]
    fn create_child_commit() {
        let (_dir, repo) = setup();
        let files = vec![("f.txt".to_string(), b"v1".to_vec())];
        let tree1 = build_initial_tree(&repo, &files).unwrap();
        let time = Utc::now();
        let c1 = create_commit(&repo, tree1, &[], "first", time).unwrap();

        let ops = vec![TreeOp::Modify {
            path: "f.txt".to_string(),
            content: b"v2".to_vec(),
        }];
        let tree2 = mutate_tree(&repo, tree1, &ops).unwrap();
        let c2 = create_commit(&repo, tree2, &[c1], "second", time).unwrap();
        let commit = repo.find_commit(c2).unwrap();
        assert_eq!(commit.parent_count(), 1);
        assert_eq!(commit.parent_id(0).unwrap(), c1);
    }

    #[test]
    fn update_branch_creates_and_moves() {
        let (_dir, repo) = setup();
        let files = vec![("f.txt".to_string(), b"v1".to_vec())];
        let tree = build_initial_tree(&repo, &files).unwrap();
        let time = Utc::now();
        let c1 = create_commit(&repo, tree, &[], "c1", time).unwrap();

        update_branch(&repo, "main", c1).unwrap();
        let branch = repo.find_branch("main", git2::BranchType::Local).unwrap();
        assert_eq!(branch.get().target().unwrap(), c1);

        let tree2 = mutate_tree(
            &repo,
            tree,
            &[TreeOp::Add {
                path: "g.txt".to_string(),
                content: b"v2".to_vec(),
            }],
        )
        .unwrap();
        let c2 = create_commit(&repo, tree2, &[c1], "c2", time).unwrap();
        update_branch(&repo, "main", c2).unwrap();
        let branch = repo.find_branch("main", git2::BranchType::Local).unwrap();
        assert_eq!(branch.get().target().unwrap(), c2);
    }

    #[test]
    fn create_lightweight_tag() {
        let (_dir, repo) = setup();
        let files = vec![("f.txt".to_string(), b"data".to_vec())];
        let tree = build_initial_tree(&repo, &files).unwrap();
        let time = Utc::now();
        let c = create_commit(&repo, tree, &[], "tagged", time).unwrap();

        create_tag(&repo, "v1.0", c, time).unwrap();
        let tag_ref = repo.find_reference("refs/tags/v1.0").unwrap();
        assert_eq!(tag_ref.target().unwrap(), c);
    }
}
