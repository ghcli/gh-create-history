use rand::Rng;

/// A single event in the planned repository history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    InitialCommit,
    Commit { branch: String },
    CreateBranch { name: String, from: String },
    Merge { source: String, target: String },
    OctopusMerge { sources: Vec<String>, target: String },
    Tag { name: String, branch: String },
    Rename { branch: String },
    Delete { branch: String },
}

/// Plans an ordered sequence of [`Event`]s that describe a realistic branch
/// topology with interleaved commits, merges, renames, deletes, and tags.
pub struct TopologyPlanner {
    commits_per_branch: u64,
    num_branches: u64,
    events: Vec<Event>,
    rng: Box<dyn RngCore>,
}

use rand::RngCore;

impl TopologyPlanner {
    pub fn new(commits_per_branch: u64, num_branches: u64, rng: &mut impl Rng) -> Self {
        // Take an independent copy of the RNG so we own it.
        let seed: u64 = rng.gen();
        use rand::SeedableRng;
        let owned_rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
        Self {
            commits_per_branch,
            num_branches,
            events: Vec::new(),
            rng: Box::new(owned_rng),
        }
    }

    /// Generate the full event plan.
    pub fn plan(&mut self) -> Vec<Event> {
        self.events.clear();
        self.events.push(Event::InitialCommit);

        let main_branch = "main".to_string();
        let mut active_branches: Vec<String> = vec![main_branch.clone()];
        let mut merged_branches: Vec<String> = Vec::new();
        let mut branch_commit_counts: std::collections::HashMap<String, u64> =
            std::collections::HashMap::new();
        branch_commit_counts.insert(main_branch.clone(), 0);

        let mut tag_counter: usize = 0;
        let mut branch_counter: usize = 0;
        let total_commits = self.commits_per_branch * self.num_branches;
        let mut total_emitted: u64 = 0;

        // We interleave: commits on existing branches, branch creation, merges,
        // tags, renames, and deletes using weighted random selection.
        while total_emitted < total_commits {
            // Safety: ensure at least one branch is active so we can always
            // emit commits and create new branches.
            if active_branches.is_empty() {
                active_branches.push(main_branch.clone());
                branch_commit_counts.entry(main_branch.clone()).or_insert(0);
            }

            let roll: f64 = self.rng.gen();

            // Create a new branch (~15 % chance while under target count).
            if roll < 0.15
                && (branch_counter as u64) < self.num_branches
                && !active_branches.is_empty()
            {
                let from = active_branches[self.rng.gen_range(0..active_branches.len())].clone();
                branch_counter += 1;
                let name = format!("feature/branch-{branch_counter}");
                self.events.push(Event::CreateBranch {
                    name: name.clone(),
                    from,
                });
                active_branches.push(name.clone());
                branch_commit_counts.insert(name, 0);
                continue;
            }

            // Merge (~8 %, only if >1 branch exists).
            if roll < 0.23 && active_branches.len() > 1 && total_emitted > 0 {
                // Pick source != main when possible.
                let src_idx = if active_branches.len() > 1 {
                    loop {
                        let i = self.rng.gen_range(0..active_branches.len());
                        if active_branches[i] != main_branch || active_branches.len() == 1 {
                            break i;
                        }
                    }
                } else {
                    0
                };
                let source = active_branches[src_idx].clone();

                // ~5 % of merges are octopus (when ≥3 branches exist).
                if self.rng.gen_bool(0.05) && active_branches.len() >= 3 {
                    let mut sources: Vec<String> = Vec::new();
                    let available = active_branches.iter().filter(|b| *b != &main_branch).count();
                    let how_many = self.rng.gen_range(2..=available.min(4));
                    let mut used = std::collections::HashSet::new();
                    used.insert(main_branch.clone());
                    while sources.len() < how_many {
                        let idx = self.rng.gen_range(0..active_branches.len());
                        let b = &active_branches[idx];
                        if !used.contains(b) {
                            sources.push(b.clone());
                            used.insert(b.clone());
                        }
                    }
                    if !sources.is_empty() {
                        self.events.push(Event::OctopusMerge {
                            sources: sources.clone(),
                            target: main_branch.clone(),
                        });
                        for s in &sources {
                            if let Some(idx) = active_branches.iter().position(|b| b == s) {
                                active_branches.remove(idx);
                            }
                            merged_branches.push(s.clone());
                        }
                    }
                } else {
                    self.events.push(Event::Merge {
                        source: source.clone(),
                        target: main_branch.clone(),
                    });
                    if source != main_branch {
                        if let Some(idx) = active_branches.iter().position(|b| b == &source) {
                            active_branches.remove(idx);
                        }
                        merged_branches.push(source);
                    }
                }
                continue;
            }

            // Tag (~5 %).
            if roll < 0.28 && total_emitted > 0 {
                let branch =
                    active_branches[self.rng.gen_range(0..active_branches.len())].clone();
                let name = format!("v{}.{}.{}", tag_counter / 100, (tag_counter / 10) % 10, tag_counter % 10);
                self.events.push(Event::Tag { name, branch });
                tag_counter += 1;
                continue;
            }

            // Rename (~3 %).
            if roll < 0.31 && active_branches.len() > 1 {
                let idx = loop {
                    let i = self.rng.gen_range(0..active_branches.len());
                    if active_branches[i] != main_branch {
                        break i;
                    }
                    if active_branches.len() == 1 {
                        break 0;
                    }
                };
                if active_branches[idx] != main_branch {
                    self.events.push(Event::Rename {
                        branch: active_branches[idx].clone(),
                    });
                    continue;
                }
            }

            // Delete (~2 %, never delete main or the last branch).
            if roll < 0.33 && active_branches.len() > 2 {
                let idx = loop {
                    let i = self.rng.gen_range(0..active_branches.len());
                    if active_branches[i] != main_branch {
                        break i;
                    }
                };
                let removed = active_branches.remove(idx);
                self.events.push(Event::Delete { branch: removed });
                continue;
            }

            // Default: emit a commit on a random active branch.
            if !active_branches.is_empty() && total_emitted < total_commits {
                let branch =
                    active_branches[self.rng.gen_range(0..active_branches.len())].clone();
                self.events.push(Event::Commit {
                    branch: branch.clone(),
                });
                *branch_commit_counts.entry(branch).or_insert(0) += 1;
                total_emitted += 1;
            }
        }

        // Final merges for any remaining non-main branches.
        let remaining: Vec<String> = active_branches
            .iter()
            .filter(|b| *b != &main_branch)
            .cloned()
            .collect();
        for source in remaining {
            self.events.push(Event::Merge {
                source,
                target: main_branch.clone(),
            });
        }

        self.events.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    fn make_plan(cpb: u64, nb: u64) -> Vec<Event> {
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let mut planner = TopologyPlanner::new(cpb, nb, &mut rng);
        planner.plan()
    }

    #[test]
    fn starts_with_initial_commit() {
        let events = make_plan(10, 3);
        assert_eq!(events[0], Event::InitialCommit);
    }

    #[test]
    fn has_commits() {
        let events = make_plan(10, 3);
        let commit_count = events
            .iter()
            .filter(|e| matches!(e, Event::Commit { .. }))
            .count();
        assert!(commit_count > 0, "plan should contain commits");
    }

    #[test]
    fn branches_created_before_their_commits() {
        let events = make_plan(20, 5);
        let mut created: std::collections::HashSet<String> = std::collections::HashSet::new();
        created.insert("main".to_string());

        for event in &events {
            match event {
                Event::CreateBranch { name, from } => {
                    assert!(
                        created.contains(from),
                        "branch {name} forked from unknown {from}"
                    );
                    created.insert(name.clone());
                }
                Event::Commit { branch } => {
                    assert!(
                        created.contains(branch),
                        "commit on uncreated branch {branch}"
                    );
                }
                _ => {}
            }
        }
    }

    #[test]
    fn no_commits_after_merge_on_source() {
        let events = make_plan(20, 5);
        let mut merged: std::collections::HashSet<String> = std::collections::HashSet::new();

        for event in &events {
            match event {
                Event::Merge { source, .. } => {
                    merged.insert(source.clone());
                }
                Event::OctopusMerge { sources, .. } => {
                    for s in sources {
                        merged.insert(s.clone());
                    }
                }
                Event::Commit { branch } => {
                    assert!(
                        !merged.contains(branch),
                        "commit on already-merged branch {branch}"
                    );
                }
                _ => {}
            }
        }
    }

    #[test]
    fn tag_and_merge_counts_reasonable() {
        let events = make_plan(50, 10);
        let tags = events.iter().filter(|e| matches!(e, Event::Tag { .. })).count();
        let merges = events
            .iter()
            .filter(|e| matches!(e, Event::Merge { .. } | Event::OctopusMerge { .. }))
            .count();
        assert!(tags > 0, "should have at least one tag");
        assert!(merges > 0, "should have at least one merge");
    }
}
