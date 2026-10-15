use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{Duration, Instant};

/// Token-bucket bandwidth throttler.
///
/// Controls the global download speed by distributing a fixed number of
/// "byte tokens" per second. Workers call `acquire(n)` before writing
/// data — if tokens are available, the call returns immediately; otherwise
/// the task sleeps until enough tokens have been refilled.
///
/// Setting `limit_bytes_per_sec = 0` disables throttling (unlimited speed).
#[derive(Debug)]
pub struct BandwidthThrottler {
    limit_bps: Arc<AtomicU64>,
    inner: Arc<Mutex<ThrottlerState>>,
}

#[derive(Debug)]
struct ThrottlerState {
    /// Maximum bytes per second (0 = unlimited).
    limit_bps: u64,
    /// Current available tokens.
    tokens: f64,
    /// Last time tokens were refilled.
    last_refill: Instant,
}

impl BandwidthThrottler {
    /// Create a new throttler with the given limit in bytes/sec.
    /// Pass 0 for unlimited bandwidth.
    pub fn new(limit_bytes_per_sec: u64) -> Self {
        Self {
            limit_bps: Arc::new(AtomicU64::new(limit_bytes_per_sec)),
            inner: Arc::new(Mutex::new(ThrottlerState {
                limit_bps: limit_bytes_per_sec,
                tokens: limit_bytes_per_sec as f64,
                last_refill: Instant::now(),
            })),
        }
    }

    /// Acquire `bytes` worth of bandwidth tokens.
    ///
    /// If the throttler is unlimited (0), returns immediately via lock-free atomic check.
    /// Otherwise, waits until enough tokens are available.
    pub async fn acquire(&self, bytes: usize) {
        // Fast path: Zero lock overhead when speed is unlimited
        if self.limit_bps.load(Ordering::Relaxed) == 0 {
            return;
        }

        let mut state = self.inner.lock().await;

        // Unlimited mode check inside lock
        if state.limit_bps == 0 {
            return;
        }

        // Refill tokens based on elapsed time
        let now = Instant::now();
        let elapsed = now.duration_since(state.last_refill).as_secs_f64();
        state.tokens += elapsed * state.limit_bps as f64;
        state.tokens = state.tokens.min(state.limit_bps as f64 * 2.0); // Cap at 2x burst
        state.last_refill = now;

        let needed = bytes as f64;
        if state.tokens >= needed {
            state.tokens -= needed;
            return;
        }

        // Calculate how long to wait for enough tokens
        let deficit = needed - state.tokens;
        let wait_secs = deficit / state.limit_bps as f64;
        state.tokens = 0.0;

        // Release the lock before sleeping
        drop(state);
        tokio::time::sleep(Duration::from_secs_f64(wait_secs)).await;

        // After waking, deduct the tokens we waited for
        let mut state = self.inner.lock().await;
        let now = Instant::now();
        let elapsed = now.duration_since(state.last_refill).as_secs_f64();
        state.tokens += elapsed * state.limit_bps as f64;
        state.tokens = state.tokens.min(state.limit_bps as f64 * 2.0);
        state.last_refill = now;
        state.tokens -= needed.min(state.tokens);
    }

    /// Update the bandwidth limit. Pass 0 for unlimited.
    pub async fn set_limit(&self, limit_bytes_per_sec: u64) {
        self.limit_bps.store(limit_bytes_per_sec, Ordering::Relaxed);
        let mut state = self.inner.lock().await;
        state.limit_bps = limit_bytes_per_sec;
        state.tokens = limit_bytes_per_sec as f64;
        state.last_refill = Instant::now();
    }

    /// Get the current limit in bytes/sec (0 = unlimited).
    pub async fn get_limit(&self) -> u64 {
        self.limit_bps.load(Ordering::Relaxed)
    }
}

impl Clone for BandwidthThrottler {
    fn clone(&self) -> Self {
        Self {
            limit_bps: Arc::clone(&self.limit_bps),
            inner: Arc::clone(&self.inner),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_unlimited_throttler() {
        let throttler = BandwidthThrottler::new(0);
        // Should return immediately without blocking
        throttler.acquire(1_000_000).await;
    }

    #[tokio::test]
    async fn test_throttler_set_limit() {
        let throttler = BandwidthThrottler::new(0);
        assert_eq!(throttler.get_limit().await, 0);

        throttler.set_limit(1_048_576).await;
        assert_eq!(throttler.get_limit().await, 1_048_576);
    }
}
