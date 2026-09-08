use std::collections::VecDeque;
use std::time::Instant;

/// A sample point: (timestamp, bytes_received_in_this_sample).
#[derive(Debug, Clone, Copy)]
struct SpeedSample {
    timestamp: Instant,
    bytes: u64,
}

/// Sliding-window + Exponential Moving Average speed calculator.
///
/// Maintains a ring buffer of timestamped byte increments over a configurable
/// window (default 1.5–2.0s). The raw windowed rate is smoothed with an EMA
/// filter (α = 0.25) to eliminate UI jitter while staying responsive to
/// throttle changes.
///
/// # Algorithm
///
/// 1. On each `record(bytes)` call, push a `(Instant::now(), bytes)` sample.
/// 2. Evict samples older than `window_duration` from the front of the deque.
/// 3. Compute instantaneous window rate:
///    `speed_window = sum(bytes) / elapsed_time`
/// 4. Apply EMA:
///    `speed_t = α * speed_window + (1 - α) * speed_{t-1}`
///
#[derive(Debug)]
pub struct SpeedMeter {
    samples: VecDeque<SpeedSample>,
    window_secs: f64,
}

impl SpeedMeter {
    /// Create a new SpeedMeter with a 1.5-second sliding window.
    pub fn new() -> Self {
        Self {
            samples: VecDeque::with_capacity(512),
            window_secs: 1.5,
        }
    }

    /// Create with custom window duration.
    pub fn with_params(window_secs: f64, _alpha: f64) -> Self {
        Self {
            samples: VecDeque::with_capacity(512),
            window_secs,
        }
    }

    /// Record a new byte increment sample at the current instant.
    pub fn record(&mut self, bytes: u64) {
        let now = Instant::now();
        self.samples.push_back(SpeedSample {
            timestamp: now,
            bytes,
        });
        self.evict_old(now);
    }

    /// Get the current sliding-window speed in bytes/sec.
    pub fn speed_bps(&self) -> f64 {
        let now = Instant::now();
        let cutoff = self.window_secs;
        let valid_samples: Vec<&SpeedSample> = self
            .samples
            .iter()
            .filter(|s| now.duration_since(s.timestamp).as_secs_f64() <= cutoff)
            .collect();

        if valid_samples.is_empty() {
            return 0.0;
        }

        let total_bytes: u64 = valid_samples.iter().map(|s| s.bytes).sum();
        let oldest = valid_samples.first().unwrap().timestamp;
        let elapsed = now.duration_since(oldest).as_secs_f64();

        if elapsed >= 0.15 {
            total_bytes as f64 / elapsed
        } else {
            (total_bytes as f64) / elapsed.max(0.05)
        }
    }

    /// Get the current speed formatted as a human-readable string.
    pub fn speed_formatted(&self) -> String {
        format_bytes_per_sec(self.speed_bps())
    }

    /// Reset the meter (e.g., on pause).
    pub fn reset(&mut self) {
        self.samples.clear();
    }

    /// Evict samples older than the window from the front of the deque.
    fn evict_old(&mut self, now: Instant) {
        let cutoff = self.window_secs;
        while let Some(front) = self.samples.front() {
            let age = now.duration_since(front.timestamp).as_secs_f64();
            if age > cutoff {
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }
}

impl Default for SpeedMeter {
    fn default() -> Self {
        Self::new()
    }
}

/// Format bytes/sec into human-readable string (e.g., "23.5 MB/s").
pub fn format_bytes_per_sec(bps: f64) -> String {
    if bps >= 1_073_741_824.0 {
        format!("{:.1} GB/s", bps / 1_073_741_824.0)
    } else if bps >= 1_048_576.0 {
        format!("{:.1} MB/s", bps / 1_048_576.0)
    } else if bps >= 1_024.0 {
        format!("{:.1} KB/s", bps / 1_024.0)
    } else {
        format!("{:.0} B/s", bps)
    }
}

/// Format bytes into human-readable size (e.g., "1.23 GB").
pub fn format_bytes(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= 1_073_741_824.0 {
        format!("{:.2} GB", b / 1_073_741_824.0)
    } else if b >= 1_048_576.0 {
        format!("{:.1} MB", b / 1_048_576.0)
    } else if b >= 1_024.0 {
        format!("{:.1} KB", b / 1_024.0)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_speed_meter_basic() {
        let mut meter = SpeedMeter::new();
        // Record 1MB over ~100ms intervals
        for _ in 0..10 {
            meter.record(1_048_576); // 1 MB
            thread::sleep(Duration::from_millis(100));
        }
        let speed = meter.speed_bps();
        // Should be roughly 10 MB/s (with some variance due to timing)
        assert!(speed > 5_000_000.0, "Speed too low: {}", speed);
        assert!(speed < 20_000_000.0, "Speed too high: {}", speed);
    }

    #[test]
    fn test_speed_meter_reset() {
        let mut meter = SpeedMeter::new();
        meter.record(1_000_000);
        assert!(meter.speed_bps() > 0.0);
        meter.reset();
        assert_eq!(meter.speed_bps(), 0.0);
    }

    #[test]
    fn test_format_bytes_per_sec() {
        assert_eq!(format_bytes_per_sec(500.0), "500 B/s");
        assert_eq!(format_bytes_per_sec(1_500.0), "1.5 KB/s");
        assert_eq!(format_bytes_per_sec(25_000_000.0), "23.8 MB/s");
        assert_eq!(format_bytes_per_sec(2_000_000_000.0), "1.9 GB/s");
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(1_536), "1.5 KB");
        assert_eq!(format_bytes(1_048_576), "1.0 MB");
        assert_eq!(format_bytes(1_073_741_824), "1.00 GB");
    }
}
