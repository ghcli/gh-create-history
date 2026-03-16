use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(
    name = "gh-create-history",
    about = "Generate synthetic git history for load testing"
)]
pub struct Args {
    /// Commits per branch
    #[arg(long, default_value_t = 1000)]
    pub commits: u64,

    /// Number of branches to create
    #[arg(long, default_value_t = 100)]
    pub branches: u64,

    /// Max file size per blob (e.g. "512b", "1kb", "10mb")
    #[arg(long, default_value = "1kb")]
    pub size: String,

    /// How far back to spread commits (e.g. "1yr", "6mo", "30d")
    #[arg(long, default_value = "1yr")]
    pub oldest: String,

    /// Push all refs to origin
    #[arg(long, default_value_t = false)]
    pub push: bool,

    /// RNG seed for reproducible history
    #[arg(long)]
    pub seed: Option<u64>,

    /// Files to touch per commit (omit for random 1-5)
    #[arg(long)]
    pub files: Option<u64>,

    /// Path to the git repository (defaults to cwd)
    #[arg(long)]
    pub repo_path: Option<PathBuf>,

    /// Suppress progress output
    #[arg(long, default_value_t = false)]
    pub quiet: bool,
}

impl Args {
    /// Parse the `--size` string into bytes.
    /// Supports suffixes: b, kb, mb, gb (case-insensitive).
    pub fn max_size_bytes(&self) -> anyhow::Result<u64> {
        parse_size(&self.size)
    }

    /// Parse the `--oldest` string into a `chrono::Duration`.
    /// Supports: yr/year/years, mo/month/months, w/week/weeks, d/day/days.
    pub fn oldest_duration(&self) -> anyhow::Result<chrono::Duration> {
        parse_duration(&self.oldest)
    }

    /// Return `--repo-path` or the current working directory.
    pub fn repo_path(&self) -> PathBuf {
        self.repo_path
            .clone()
            .unwrap_or_else(|| std::env::current_dir().expect("cannot determine cwd"))
    }

    /// If `--files` is set return `(n, n)`; otherwise `(1, 5)` for a random range.
    pub fn files_per_commit(&self) -> (u64, u64) {
        match self.files {
            Some(n) => (n, n),
            None => (1, 5),
        }
    }
}

fn parse_size(s: &str) -> anyhow::Result<u64> {
    let s = s.trim().to_lowercase();
    if s.is_empty() {
        anyhow::bail!("empty size string");
    }

    let (digits, suffix) = split_number_suffix(&s)?;
    let n: u64 = digits
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid number in size: {s}"))?;

    if n == 0 {
        anyhow::bail!("size must be greater than zero: {s}");
    }

    let multiplier: u64 = match suffix {
        "b" => 1,
        "kb" => 1024,
        "mb" => 1024 * 1024,
        "gb" => 1024 * 1024 * 1024,
        other => anyhow::bail!("unknown size suffix: {other}"),
    };

    Ok(n * multiplier)
}

fn parse_duration(s: &str) -> anyhow::Result<chrono::Duration> {
    let s = s.trim().to_lowercase();
    if s.is_empty() {
        anyhow::bail!("empty duration string");
    }

    let (digits, suffix) = split_number_suffix(&s)?;
    let n: i64 = digits
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid number in duration: {s}"))?;

    if n <= 0 {
        anyhow::bail!("duration must be positive: {s}");
    }

    let days = match suffix {
        "yr" | "year" | "years" => n * 365,
        "mo" | "month" | "months" => n * 30,
        "w" | "week" | "weeks" => n * 7,
        "d" | "day" | "days" => n,
        other => anyhow::bail!("unknown duration suffix: {other}"),
    };

    chrono::Duration::try_days(days)
        .ok_or_else(|| anyhow::anyhow!("duration out of range: {days} days"))
}

/// Split a string like "10kb" into ("10", "kb").
fn split_number_suffix(s: &str) -> anyhow::Result<(&str, &str)> {
    let first_alpha = s
        .find(|c: char| c.is_ascii_alphabetic())
        .ok_or_else(|| anyhow::anyhow!("missing suffix in: {s}"))?;

    if first_alpha == 0 {
        anyhow::bail!("missing number in: {s}");
    }

    let (digits, suffix) = s.split_at(first_alpha);

    // Reject negative numbers embedded as text (the leading '-' would have been
    // caught by clap, but guard against direct API use).
    if digits.starts_with('-') {
        anyhow::bail!("value must be positive: {s}");
    }

    Ok((digits, suffix))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- size parsing ----

    #[test]
    fn size_1kb() {
        assert_eq!(parse_size("1kb").unwrap(), 1024);
    }

    #[test]
    fn size_10mb() {
        assert_eq!(parse_size("10mb").unwrap(), 10_485_760);
    }

    #[test]
    fn size_512b() {
        assert_eq!(parse_size("512b").unwrap(), 512);
    }

    #[test]
    fn size_1gb() {
        assert_eq!(parse_size("1gb").unwrap(), 1_073_741_824);
    }

    #[test]
    fn size_case_insensitive() {
        assert_eq!(parse_size("1KB").unwrap(), 1024);
        assert_eq!(parse_size("10MB").unwrap(), 10_485_760);
    }

    #[test]
    fn size_zero_rejected() {
        assert!(parse_size("0kb").is_err());
    }

    #[test]
    fn size_no_suffix_rejected() {
        assert!(parse_size("100").is_err());
    }

    #[test]
    fn size_alpha_only_rejected() {
        assert!(parse_size("abc").is_err());
    }

    #[test]
    fn size_empty_rejected() {
        assert!(parse_size("").is_err());
    }

    // ---- duration parsing ----

    #[test]
    fn duration_1yr() {
        let d = parse_duration("1yr").unwrap();
        assert_eq!(d.num_days(), 365);
    }

    #[test]
    fn duration_6mo() {
        let d = parse_duration("6mo").unwrap();
        assert_eq!(d.num_days(), 180);
    }

    #[test]
    fn duration_30d() {
        let d = parse_duration("30d").unwrap();
        assert_eq!(d.num_days(), 30);
    }

    #[test]
    fn duration_2w() {
        let d = parse_duration("2w").unwrap();
        assert_eq!(d.num_days(), 14);
    }

    #[test]
    fn duration_long_suffixes() {
        assert_eq!(parse_duration("1year").unwrap().num_days(), 365);
        assert_eq!(parse_duration("2years").unwrap().num_days(), 730);
        assert_eq!(parse_duration("1month").unwrap().num_days(), 30);
        assert_eq!(parse_duration("3months").unwrap().num_days(), 90);
        assert_eq!(parse_duration("1week").unwrap().num_days(), 7);
        assert_eq!(parse_duration("2weeks").unwrap().num_days(), 14);
        assert_eq!(parse_duration("1day").unwrap().num_days(), 1);
        assert_eq!(parse_duration("5days").unwrap().num_days(), 5);
    }

    #[test]
    fn duration_negative_rejected() {
        assert!(parse_duration("-1yr").is_err());
    }

    #[test]
    fn duration_empty_rejected() {
        assert!(parse_duration("").is_err());
    }

    #[test]
    fn duration_alpha_only_rejected() {
        assert!(parse_duration("abc").is_err());
    }

    // ---- defaults via parse_from ----

    #[test]
    fn defaults() {
        let args = Args::parse_from(["test"]);
        assert_eq!(args.commits, 1000);
        assert_eq!(args.branches, 100);
        assert_eq!(args.size, "1kb");
        assert_eq!(args.oldest, "1yr");
        assert!(!args.push);
        assert!(!args.quiet);
        assert!(args.seed.is_none());
        assert!(args.files.is_none());
        assert!(args.repo_path.is_none());
    }

    #[test]
    fn files_per_commit_default() {
        let args = Args::parse_from(["test"]);
        assert_eq!(args.files_per_commit(), (1, 5));
    }

    #[test]
    fn files_per_commit_explicit() {
        let args = Args::parse_from(["test", "--files", "3"]);
        assert_eq!(args.files_per_commit(), (3, 3));
    }

    #[test]
    fn max_size_bytes_method() {
        let args = Args::parse_from(["test", "--size", "10mb"]);
        assert_eq!(args.max_size_bytes().unwrap(), 10_485_760);
    }

    #[test]
    fn oldest_duration_method() {
        let args = Args::parse_from(["test", "--oldest", "6mo"]);
        assert_eq!(args.oldest_duration().unwrap().num_days(), 180);
    }
}
