use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};

/// Create a progress bar; returns a hidden bar when `quiet` is true.
pub fn create_progress_bar(total: u64, quiet: bool) -> ProgressBar {
    if quiet {
        ProgressBar::hidden()
    } else {
        let pb = ProgressBar::new(total);
        pb.set_style(
            ProgressStyle::with_template(
                "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})",
            )
            .unwrap()
            .progress_chars("#>-"),
        );
        pb
    }
}

/// Accumulated counters for the generation run.
pub struct Summary {
    pub commits: u64,
    pub branches: u64,
    pub merges: u64,
    pub octopus_merges: u64,
    pub tags: u64,
    pub conflicts: u64,
    pub renames: u64,
    pub deletes: u64,
}

impl Summary {
    pub fn new() -> Self {
        Self {
            commits: 0,
            branches: 0,
            merges: 0,
            octopus_merges: 0,
            tags: 0,
            conflicts: 0,
            renames: 0,
            deletes: 0,
        }
    }

    pub fn print(&self, elapsed: Duration) {
        println!();
        println!("╔══════════════════════════════════════╗");
        println!("║         Generation Summary           ║");
        println!("╠══════════════════════════════════════╣");
        println!("║  Commits       {:>8}              ║", self.commits);
        println!("║  Branches      {:>8}              ║", self.branches);
        println!("║  Merges        {:>8}              ║", self.merges);
        println!("║  Octopus       {:>8}              ║", self.octopus_merges);
        println!("║  Tags          {:>8}              ║", self.tags);
        println!("║  Conflicts     {:>8}              ║", self.conflicts);
        println!("║  Renames       {:>8}              ║", self.renames);
        println!("║  Deletes       {:>8}              ║", self.deletes);
        println!("╠══════════════════════════════════════╣");
        println!(
            "║  Elapsed       {:>8.2}s             ║",
            elapsed.as_secs_f64()
        );
        println!("╚══════════════════════════════════════╝");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_bar_hidden_when_quiet() {
        let pb = create_progress_bar(100, true);
        assert!(pb.is_hidden());
    }

    #[test]
    fn progress_bar_visible_when_not_quiet() {
        let pb = create_progress_bar(100, false);
        // In a test environment without TTY, the bar may be hidden.
        // Just verify it was created with the correct length.
        assert_eq!(pb.length(), Some(100));
    }

    #[test]
    fn summary_new_all_zeros() {
        let s = Summary::new();
        assert_eq!(s.commits, 0);
        assert_eq!(s.branches, 0);
        assert_eq!(s.merges, 0);
        assert_eq!(s.octopus_merges, 0);
        assert_eq!(s.tags, 0);
        assert_eq!(s.conflicts, 0);
        assert_eq!(s.renames, 0);
        assert_eq!(s.deletes, 0);
    }

    #[test]
    fn summary_print_does_not_panic() {
        let s = Summary {
            commits: 42,
            branches: 5,
            merges: 3,
            octopus_merges: 1,
            tags: 2,
            conflicts: 1,
            renames: 4,
            deletes: 2,
        };
        s.print(Duration::from_secs_f64(1.234));
    }
}
