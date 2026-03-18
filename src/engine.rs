use std::collections::{HashMap, HashSet};
use std::time::Instant;

use anyhow::Result;
use chrono::{TimeZone, Utc};
use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use crate::cli::Args;
use crate::content::ContentGenerator;
use crate::git_ops::{self, TreeOp};
use crate::merge;
use crate::progress::{self, Summary};
use crate::timestamps::TimestampGenerator;
use crate::topology::{Event, TopologyPlanner};

/// Main entry-point: generate synthetic git history according to `args`.
pub fn run(args: Args) -> Result<()> {
    let start = Instant::now();

    // ── configuration ────────────────────────────────────────────────────
    let repo_path = args.repo_path();
    let max_size = args.max_size_bytes()?;
    let oldest = args.oldest_duration()?;
    let (min_files, max_files) = args.files_per_commit();

    let repo = git_ops::init_or_open_repo(&repo_path)?;

    // ── RNG — deterministic when --seed is supplied ──────────────────────
    let mut rng: ChaCha8Rng = match args.seed {
        Some(s) => ChaCha8Rng::seed_from_u64(s),
        None => ChaCha8Rng::from_entropy(),
    };

    // ── generators ───────────────────────────────────────────────────────
    let mut content_gen = ContentGenerator::new(args.seed);
    let mut planner = TopologyPlanner::new(args.commits, args.branches, &mut rng);
    let events = planner.plan();

    // Fixed reference time for reproducibility when a seed is given.
    let mut ts_gen = if args.seed.is_some() {
        let end = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
        let start_ts = end - oldest;
        TimestampGenerator::with_range(start_ts, end, events.len() as u64, &mut rng)
    } else {
        TimestampGenerator::new(oldest, events.len() as u64, &mut rng)
    };

    // ── state ────────────────────────────────────────────────────────────
    let pb = progress::create_progress_bar(events.len() as u64, args.quiet);
    let mut branch_heads: HashMap<String, git2::Oid> = HashMap::new();
    let mut branch_trees: HashMap<String, git2::Oid> = HashMap::new();
    let mut branch_files: HashMap<String, HashSet<String>> = HashMap::new();
    let mut summary = Summary::new();

    // ── event loop ───────────────────────────────────────────────────────
    for event in &events {
        let time = ts_gen.next();

        match event {
            // ── initial commit on main ───────────────────────────────
            Event::InitialCommit => {
                let n = rng.gen_range(min_files..=max_files) as usize;
                let files: Vec<(String, Vec<u8>)> = (0..n)
                    .map(|_| {
                        let p = content_gen.generate_file_path();
                        let c = content_gen.generate_file_content(max_size);
                        (p, c)
                    })
                    .collect();

                let tree_oid = git_ops::build_initial_tree(&repo, &files)?;
                let commit_oid =
                    git_ops::create_commit(&repo, tree_oid, &[], "Initial commit", time)?;
                git_ops::update_branch(&repo, "main", commit_oid)?;

                let names: HashSet<String> = files.iter().map(|(p, _)| p.clone()).collect();
                branch_heads.insert("main".to_string(), commit_oid);
                branch_trees.insert("main".to_string(), tree_oid);
                branch_files.insert("main".to_string(), names);
                summary.commits += 1;
            }

            // ── branch creation (fork from existing branch) ──────────
            Event::CreateBranch { name, from } => {
                if let (Some(&from_head), Some(&from_tree)) =
                    (branch_heads.get(from), branch_trees.get(from))
                {
                    // Add a small initial commit to differentiate the branch.
                    let n = rng.gen_range(min_files..=max_files) as usize;
                    let mut ops = Vec::with_capacity(n);
                    let mut new_files: HashSet<String> = branch_files
                        .get(from)
                        .cloned()
                        .unwrap_or_default();

                    for _ in 0..n {
                        let p = content_gen.generate_file_path();
                        let c = content_gen.generate_file_content(max_size);
                        ops.push(TreeOp::Add {
                            path: p.clone(),
                            content: c,
                        });
                        new_files.insert(p);
                    }

                    let tree_oid = git_ops::mutate_tree(&repo, from_tree, &ops)?;
                    let msg = format!("Initial commit on {name}");
                    let commit_oid =
                        git_ops::create_commit(&repo, tree_oid, &[from_head], &msg, time)?;
                    git_ops::update_branch(&repo, name, commit_oid)?;

                    branch_heads.insert(name.clone(), commit_oid);
                    branch_trees.insert(name.clone(), tree_oid);
                    branch_files.insert(name.clone(), new_files);
                    summary.branches += 1;
                }
            }

            // ── regular commit ───────────────────────────────────────
            Event::Commit { branch } => {
                if let (Some(&head), Some(&tree)) =
                    (branch_heads.get(branch), branch_trees.get(branch))
                {
                    let n = rng.gen_range(min_files..=max_files) as usize;
                    let mut ops = Vec::with_capacity(n);
                    let file_set = branch_files.entry(branch.clone()).or_default();

                    for _ in 0..n {
                        let path = content_gen.generate_file_path();
                        let content = content_gen.generate_file_content(max_size);
                        ops.push(TreeOp::Add {
                            path: path.clone(),
                            content,
                        });
                        file_set.insert(path);
                    }

                    let new_tree = git_ops::mutate_tree(&repo, tree, &ops)?;
                    let msg = content_gen.generate_commit_message();
                    let commit_oid =
                        git_ops::create_commit(&repo, new_tree, &[head], &msg, time)?;
                    git_ops::update_branch(&repo, branch, commit_oid)?;

                    branch_heads.insert(branch.clone(), commit_oid);
                    branch_trees.insert(branch.clone(), new_tree);
                    summary.commits += 1;
                }
            }

            // ── 2-parent merge (10 % chance of simulated conflict) ───
            Event::Merge { source, target } => {
                if branch_heads.contains_key(source) && branch_heads.contains_key(target) {
                    let is_conflict = rng.gen_bool(0.1);
                    let commit_oid = if is_conflict {
                        summary.conflicts += 1;
                        merge::create_conflict_merge(
                            &repo,
                            source,
                            target,
                            time,
                            &mut content_gen,
                            max_size,
                        )?
                    } else {
                        merge::create_merge_commit(
                            &repo,
                            source,
                            target,
                            time,
                            &mut content_gen,
                            max_size,
                        )?
                    };
                    let commit = repo.find_commit(commit_oid)?;
                    branch_heads.insert(target.clone(), commit_oid);
                    branch_trees.insert(target.clone(), commit.tree_id());
                    summary.merges += 1;
                }
            }

            // ── octopus merge ────────────────────────────────────────
            Event::OctopusMerge { sources, target } => {
                let all_exist = sources.iter().all(|s| branch_heads.contains_key(s))
                    && branch_heads.contains_key(target);
                if all_exist {
                    let commit_oid = merge::create_octopus_merge(
                        &repo,
                        sources,
                        target,
                        time,
                        &mut content_gen,
                        max_size,
                    )?;
                    let commit = repo.find_commit(commit_oid)?;
                    branch_heads.insert(target.clone(), commit_oid);
                    branch_trees.insert(target.clone(), commit.tree_id());
                    summary.octopus_merges += 1;
                }
            }

            // ── lightweight tag ──────────────────────────────────────
            Event::Tag { name, branch } => {
                if let Some(&head) = branch_heads.get(branch) {
                    git_ops::create_tag(&repo, name, head, time)?;
                    summary.tags += 1;
                }
            }

            // ── file rename ──────────────────────────────────────────
            Event::Rename { branch } => {
                if let (Some(&head), Some(&tree)) =
                    (branch_heads.get(branch), branch_trees.get(branch))
                {
                    let file_set = branch_files.entry(branch.clone()).or_default();
                    if !file_set.is_empty() {
                        let files_vec: Vec<String> = file_set.iter().cloned().collect();
                        let idx = rng.gen_range(0..files_vec.len());
                        let old_path = files_vec[idx].clone();
                        let new_path = content_gen.generate_file_path();

                        let ops = vec![TreeOp::Rename {
                            old_path: old_path.clone(),
                            new_path: new_path.clone(),
                        }];
                        let new_tree = git_ops::mutate_tree(&repo, tree, &ops)?;
                        let msg = format!("Rename {old_path} → {new_path}");
                        let commit_oid =
                            git_ops::create_commit(&repo, new_tree, &[head], &msg, time)?;
                        git_ops::update_branch(&repo, branch, commit_oid)?;

                        file_set.remove(&old_path);
                        file_set.insert(new_path);
                        branch_heads.insert(branch.clone(), commit_oid);
                        branch_trees.insert(branch.clone(), new_tree);
                        summary.renames += 1;
                    }
                }
            }

            // ── file delete (branch removed from active topology) ────
            Event::Delete { branch } => {
                if let (Some(&head), Some(&tree)) =
                    (branch_heads.get(branch), branch_trees.get(branch))
                {
                    let file_set = branch_files.entry(branch.clone()).or_default();
                    if file_set.len() > 1 {
                        let files_vec: Vec<String> = file_set.iter().cloned().collect();
                        let idx = rng.gen_range(0..files_vec.len());
                        let path = files_vec[idx].clone();

                        let ops = vec![TreeOp::Delete { path: path.clone() }];
                        let new_tree = git_ops::mutate_tree(&repo, tree, &ops)?;
                        let msg = format!("Delete {path}");
                        let commit_oid =
                            git_ops::create_commit(&repo, new_tree, &[head], &msg, time)?;
                        git_ops::update_branch(&repo, branch, commit_oid)?;

                        file_set.remove(&path);
                        branch_heads.insert(branch.clone(), commit_oid);
                        branch_trees.insert(branch.clone(), new_tree);
                    }
                }
                // The topology planner removed this branch from active set.
                branch_heads.remove(branch);
                branch_trees.remove(branch);
                branch_files.remove(branch);
                summary.deletes += 1;
            }
        }

        pb.inc(1);
    }

    pb.finish_and_clear();

    // ── push ─────────────────────────────────────────────────────────────
    if args.push {
        git_ops::push_all(&repo)?;
    }

    if !args.quiet {
        summary.print(start.elapsed());
    }

    Ok(())
}
