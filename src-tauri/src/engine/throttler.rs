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

    /// Acquire `bytes` worth of bandwidth tokens (non-cancellable convenience helper).
    #[cfg(test)]
    pub async fn acquire(&self, bytes: usize) {
        let wait = self.reserve_wait(bytes).await;
        if let Some(dur) = wait {
            tokio::time::sleep(dur).await;
        }
    }

    /// Cancellable version of `acquire`: returns `false` immediately if `cancel_rx` fires while waiting.
    pub async fn acquire_cancellable(
        &self,
        bytes: usize,
        cancel_rx: &mut tokio::sync::watch::Receiver<bool>,
    ) -> bool {
        if *cancel_rx.borrow() {
            return false;
        }
        let wait = self.reserve_wait(bytes).await;
        if let Some(dur) = wait {
            tokio::select! {
                _ = tokio::time::sleep(dur) => true,
                _ = cancel_rx.changed() => !*cancel_rx.borrow(),
            }
        } else {
            true
        }
    }

    async fn reserve_wait(&self, bytes: usize) -> Option<Duration> {
        // Fast path: Zero lock overhead when speed is unlimited
        if self.limit_bps.load(Ordering::Relaxed) == 0 {
            return None;
        }

        let mut state = self.inner.lock().await;
        if state.limit_bps == 0 {
            return None;
        }

        // Refill tokens based on elapsed time (cap positive burst at 1s worth of limit)
        let now = Instant::now();
        let elapsed = now.duration_since(state.last_refill).as_secs_f64();
        let limit = state.limit_bps as f64;
        state.tokens = (state.tokens + elapsed * limit).min(limit);
        state.last_refill = now;

        // Reserve needed bytes up front; negative tokens represent queued debt for subsequent workers
        state.tokens -= bytes as f64;
        if state.tokens >= 0.0 {
            None
        } else {
            let wait_secs = (-state.tokens) / limit;
            Some(Duration::from_secs_f64(wait_secs.min(10.0)))
        }
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
