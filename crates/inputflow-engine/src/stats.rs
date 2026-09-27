//! Bounded reservoir for recording latency samples and computing percentiles.
//!
//! The platform layer samples callback duration and hold delay through this
//! tracker. It keeps the most recent `capacity` samples (a sliding window) so
//! memory stays bounded regardless of how many events are processed, and it
//! computes p50/p95/p99 with the nearest-rank method.

/// A fixed-capacity sliding window of `u64` samples with percentile queries.
#[derive(Debug, Clone)]
pub struct PercentileTracker {
    capacity: usize,
    samples: Vec<u64>,
    next: usize,
    total: u64,
}

/// An immutable copy of a tracker window. It can be sorted and summarized
/// without holding the recorder's mutex.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PercentileSnapshot {
    total: u64,
    samples: Vec<u64>,
}

/// One-pass summary of a percentile snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PercentileSummary {
    pub total: u64,
    pub retained: usize,
    pub p50: Option<u64>,
    pub p95: Option<u64>,
    pub p99: Option<u64>,
    pub max: Option<u64>,
}

impl PercentileTracker {
    /// Create an empty tracker retaining at most `capacity` samples.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            samples: Vec::with_capacity(capacity.min(1024)),
            next: 0,
            total: 0,
        }
    }

    /// Record one sample. Beyond `capacity`, the oldest retained sample is
    /// replaced, keeping memory bounded.
    pub fn record(&mut self, sample: u64) {
        self.total += 1;
        if self.samples.len() < self.capacity {
            self.samples.push(sample);
        } else {
            self.samples[self.next] = sample;
            self.next = (self.next + 1) % self.capacity;
        }
    }

    /// Number of samples currently retained.
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Whether no samples have been recorded yet.
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Total number of samples ever recorded (may exceed [`Self::len`]).
    pub fn total(&self) -> u64 {
        self.total
    }

    /// Copy the bounded retained window. Callers should release any surrounding
    /// lock before invoking [`PercentileSnapshot::summary`].
    pub fn snapshot(&self) -> PercentileSnapshot {
        PercentileSnapshot {
            total: self.total,
            samples: self.samples.clone(),
        }
    }

    /// Nearest-rank percentile in `[0, 100]`; `None` when no samples.
    pub fn percentile(&self, p: f64) -> Option<u64> {
        if self.samples.is_empty() {
            return None;
        }
        if p <= 0.0 {
            return self.samples.iter().copied().min();
        }
        if p >= 100.0 {
            return self.samples.iter().copied().max();
        }
        let mut sorted = self.samples.clone();
        sorted.sort_unstable();
        let rank = ((p / 100.0) * sorted.len() as f64).ceil() as usize;
        Some(sorted[rank.saturating_sub(1).min(sorted.len() - 1)])
    }

    /// The 50th percentile (median) sample.
    pub fn p50(&self) -> Option<u64> {
        self.percentile(50.0)
    }

    /// The 95th percentile sample.
    pub fn p95(&self) -> Option<u64> {
        self.percentile(95.0)
    }

    /// The 99th percentile sample.
    pub fn p99(&self) -> Option<u64> {
        self.percentile(99.0)
    }
}

impl PercentileSnapshot {
    /// Sort this snapshot once and calculate every reported percentile.
    pub fn summary(mut self) -> PercentileSummary {
        self.samples.sort_unstable();
        PercentileSummary {
            total: self.total,
            retained: self.samples.len(),
            p50: nearest_rank(&self.samples, 50.0),
            p95: nearest_rank(&self.samples, 95.0),
            p99: nearest_rank(&self.samples, 99.0),
            max: self.samples.last().copied(),
        }
    }
}

fn nearest_rank(sorted: &[u64], p: f64) -> Option<u64> {
    if sorted.is_empty() {
        return None;
    }
    if p <= 0.0 {
        return sorted.first().copied();
    }
    if p >= 100.0 {
        return sorted.last().copied();
    }
    let rank = ((p / 100.0) * sorted.len() as f64).ceil() as usize;
    Some(sorted[rank.saturating_sub(1).min(sorted.len() - 1)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_tracker_has_no_percentiles() {
        let t = PercentileTracker::new(10);
        assert!(t.is_empty());
        assert_eq!(t.p50(), None);
        assert_eq!(t.p95(), None);
        assert_eq!(t.p99(), None);
        assert_eq!(t.total(), 0);
    }

    #[test]
    fn percentiles_use_nearest_rank() {
        let mut t = PercentileTracker::new(10);
        for v in [1u64, 2, 3, 4, 5] {
            t.record(v);
        }
        assert_eq!(t.len(), 5);
        assert_eq!(t.p50(), Some(3));
        // ceil(0.95 * 5) = 5 -> index 4 -> value 5.
        assert_eq!(t.p95(), Some(5));
        assert_eq!(t.p99(), Some(5));
        assert_eq!(t.percentile(0.0), Some(1));
        assert_eq!(t.percentile(100.0), Some(5));
    }

    #[test]
    fn capacity_bounds_memory_as_sliding_window() {
        let mut t = PercentileTracker::new(3);
        for v in [10u64, 20, 30, 40, 50] {
            t.record(v);
        }
        assert_eq!(t.len(), 3);
        assert_eq!(t.total(), 5);
        // The retained window is the most recent 3 samples: 30, 40, 50.
        assert_eq!(t.percentile(0.0), Some(30));
        assert_eq!(t.percentile(100.0), Some(50));
        assert_eq!(t.p50(), Some(40));
    }

    #[test]
    fn snapshot_summary_sorts_once_and_preserves_total() {
        let mut t = PercentileTracker::new(5);
        for value in [90, 10, 50, 30, 70, 110] {
            t.record(value);
        }

        assert_eq!(
            t.snapshot().summary(),
            PercentileSummary {
                total: 6,
                retained: 5,
                p50: Some(50),
                p95: Some(110),
                p99: Some(110),
                max: Some(110),
            }
        );
    }
}
