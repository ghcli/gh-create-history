use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
use rand::Rng;

/// Pre-generates a sorted sequence of realistic commit timestamps.
///
/// Distribution targets:
/// - ~70 % weekday commits
/// - ~60 % business-hours commits (09:00–17:00 UTC)
/// - Minimum 1-second gap between consecutive timestamps
/// - Monotonically increasing
pub struct TimestampGenerator {
    timestamps: Vec<DateTime<Utc>>,
    cursor: usize,
}

impl TimestampGenerator {
    /// Create timestamps spread over the last `oldest` duration.
    pub fn new(oldest: Duration, total_commits: u64, rng: &mut impl Rng) -> Self {
        let end = Utc::now();
        let start = end - oldest;
        Self::with_range(start, end, total_commits, rng)
    }

    /// Create timestamps between an explicit start and end (inclusive).
    pub fn with_range(
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        total_commits: u64,
        rng: &mut impl Rng,
    ) -> Self {
        let count = total_commits as usize;
        if count == 0 {
            return Self {
                timestamps: Vec::new(),
                cursor: 0,
            };
        }

        let span_secs = (end - start).num_seconds().max(1);
        let mut raw: Vec<i64> = Vec::with_capacity(count);

        for _ in 0..count {
            // Bias toward weekdays and business hours.
            let ts = loop {
                let candidate_offset = rng.gen_range(0..span_secs);
                let candidate = start + Duration::seconds(candidate_offset);

                let weekday = candidate.weekday().num_days_from_monday() < 5;
                let biz_hour = {
                    let h = candidate.hour();
                    h >= 9 && h < 17
                };

                // Accept with probability that favours weekday + business hours.
                let accept_prob: f64 = match (weekday, biz_hour) {
                    (true, true) => 0.95,
                    (true, false) => 0.55,
                    (false, true) => 0.30,
                    (false, false) => 0.10,
                };

                if rng.gen_bool(accept_prob) {
                    break candidate_offset;
                }
            };
            raw.push(ts);
        }

        raw.sort_unstable();

        // Enforce minimum 1-second gap (shift later timestamps forward).
        for i in 1..raw.len() {
            if raw[i] <= raw[i - 1] {
                raw[i] = raw[i - 1] + 1;
            }
        }

        // Add small jitter (±0–30 s) while preserving monotonicity.
        let mut timestamps: Vec<DateTime<Utc>> = Vec::with_capacity(count);
        for (i, &offset) in raw.iter().enumerate() {
            let jitter = rng.gen_range(0..=30);
            let adjusted = offset + jitter;

            let ts = start + Duration::seconds(adjusted);
            // Ensure monotonicity after jitter.
            if let Some(prev) = timestamps.last() {
                if ts <= *prev {
                    timestamps.push(*prev + Duration::seconds(1));
                    continue;
                }
            }
            // Also guard against exceeding end.
            if i == count - 1 && ts > end {
                timestamps.push(end);
            } else {
                timestamps.push(ts);
            }
        }

        Self {
            timestamps,
            cursor: 0,
        }
    }

    /// Return the next timestamp in the sequence.
    /// Wraps around to the beginning if exhausted.
    pub fn next(&mut self) -> DateTime<Utc> {
        if self.timestamps.is_empty() {
            return Utc::now();
        }
        let ts = self.timestamps[self.cursor];
        self.cursor = (self.cursor + 1) % self.timestamps.len();
        ts
    }

    /// Return the next `count` timestamps (advances the cursor).
    pub fn get_timestamps(&mut self, count: usize) -> Vec<DateTime<Utc>> {
        (0..count).map(|_| self.next()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    fn make_gen(total: u64) -> TimestampGenerator {
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let start = DateTime::parse_from_rfc3339("2023-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let end = DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        TimestampGenerator::with_range(start, end, total, &mut rng)
    }

    #[test]
    fn monotonically_increasing() {
        let mut gen = make_gen(200);
        let ts = gen.get_timestamps(200);
        for w in ts.windows(2) {
            assert!(w[1] > w[0], "timestamps must be strictly increasing");
        }
    }

    #[test]
    fn within_window() {
        let mut gen = make_gen(100);
        let start = DateTime::parse_from_rfc3339("2023-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let end = DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        // Allow a small overflow from jitter/gap enforcement.
        let slack = Duration::seconds(200 * 31);
        let ts = gen.get_timestamps(100);
        for t in &ts {
            assert!(*t >= start, "timestamp {t} before start");
            assert!(*t <= end + slack, "timestamp {t} too far past end");
        }
    }

    #[test]
    fn minimum_gap() {
        let mut gen = make_gen(500);
        let ts = gen.get_timestamps(500);
        for w in ts.windows(2) {
            let gap = (w[1] - w[0]).num_seconds();
            assert!(gap >= 1, "gap must be >= 1 s, got {gap}");
        }
    }

    #[test]
    fn weekday_bias() {
        let mut gen = make_gen(1000);
        let ts = gen.get_timestamps(1000);
        let weekday_count = ts
            .iter()
            .filter(|t| t.weekday().num_days_from_monday() < 5)
            .count();
        let ratio = weekday_count as f64 / ts.len() as f64;
        assert!(
            ratio > 0.55,
            "weekday ratio {ratio:.2} should be > 0.55 (target ~0.70)"
        );
    }

    #[test]
    fn empty_commits() {
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let start = Utc::now() - Duration::days(30);
        let end = Utc::now();
        let mut gen = TimestampGenerator::with_range(start, end, 0, &mut rng);
        // next() on empty returns Utc::now() — just ensure no panic.
        let _t = gen.next();
    }
}
