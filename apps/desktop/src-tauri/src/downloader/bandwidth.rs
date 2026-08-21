use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::time::sleep;

const WINDOW: Duration = Duration::from_secs(1);
const MIN_SLEEP: Duration = Duration::from_millis(1);

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct BandwidthSnapshot {
    pub limit_bytes_per_sec: Option<u64>,
    pub current_bytes_per_sec: u64,
    pub total_bytes: u64,
}

#[derive(Debug)]
struct LimiterState {
    window_started: Instant,
    window_bytes: u64,
    total_bytes: u64,
    limit_bytes_per_sec: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct BandwidthLimiter {
    state: Arc<Mutex<LimiterState>>,
}

impl Default for BandwidthLimiter {
    fn default() -> Self {
        Self::unlimited()
    }
}

impl BandwidthLimiter {
    pub fn unlimited() -> Self {
        Self::with_limit_bytes_per_sec(None)
    }

    pub fn with_limit_bytes_per_sec(limit_bytes_per_sec: Option<u64>) -> Self {
        Self {
            state: Arc::new(Mutex::new(LimiterState {
                window_started: Instant::now(),
                window_bytes: 0,
                total_bytes: 0,
                limit_bytes_per_sec,
            })),
        }
    }

    pub fn set_limit_bytes_per_sec(&self, limit_bytes_per_sec: Option<u64>) {
        if let Ok(mut state) = self.state.lock() {
            state.window_started = Instant::now();
            state.window_bytes = 0;
            state.limit_bytes_per_sec = limit_bytes_per_sec.filter(|value| *value > 0);
        }
    }

    pub fn limit_bytes_per_sec(&self) -> Option<u64> {
        self.state
            .lock()
            .ok()
            .and_then(|state| state.limit_bytes_per_sec)
    }

    pub fn max_chunk_size(&self, requested: usize) -> usize {
        self.limit_bytes_per_sec()
            .and_then(|limit| usize::try_from(limit).ok())
            .map(|limit| requested.min(limit.max(1)))
            .unwrap_or(requested)
    }

    pub async fn acquire(&self, bytes: u64) {
        if bytes == 0 {
            return;
        }
        loop {
            let wait = {
                let Ok(mut state) = self.state.lock() else {
                    return;
                };
                let now = Instant::now();
                let elapsed = now.saturating_duration_since(state.window_started);
                if elapsed >= WINDOW {
                    state.window_started = now;
                    state.window_bytes = 0;
                }
                match state.limit_bytes_per_sec {
                    None => {
                        state.total_bytes = state.total_bytes.saturating_add(bytes);
                        return;
                    }
                    Some(limit) if state.window_bytes.saturating_add(bytes) <= limit => {
                        state.window_bytes = state.window_bytes.saturating_add(bytes);
                        state.total_bytes = state.total_bytes.saturating_add(bytes);
                        return;
                    }
                    Some(_) => WINDOW
                        .saturating_sub(now.saturating_duration_since(state.window_started))
                        .max(MIN_SLEEP),
                }
            };
            sleep(wait).await;
        }
    }

    pub fn snapshot(&self) -> BandwidthSnapshot {
        let Ok(mut state) = self.state.lock() else {
            return BandwidthSnapshot {
                limit_bytes_per_sec: None,
                current_bytes_per_sec: 0,
                total_bytes: 0,
            };
        };
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(state.window_started);
        if elapsed >= WINDOW {
            state.window_started = now;
            state.window_bytes = 0;
        }
        let seconds = elapsed.as_secs_f64().max(0.001);
        BandwidthSnapshot {
            limit_bytes_per_sec: state.limit_bytes_per_sec,
            current_bytes_per_sec: (state.window_bytes as f64 / seconds).round() as u64,
            total_bytes: state.total_bytes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BandwidthLimiter;
    use std::time::Instant;

    #[tokio::test]
    async fn unlimited_limiter_records_bytes_without_waiting() {
        let limiter = BandwidthLimiter::unlimited();
        let started = Instant::now();
        limiter.acquire(32_768).await;
        assert!(started.elapsed().as_millis() < 100);
        assert_eq!(limiter.snapshot().total_bytes, 32_768);
        assert_eq!(limiter.snapshot().limit_bytes_per_sec, None);
    }

    #[tokio::test]
    async fn limited_limiter_waits_for_the_next_window() {
        let limiter = BandwidthLimiter::with_limit_bytes_per_sec(Some(1_024));
        limiter.acquire(1_024).await;
        let started = Instant::now();
        limiter.acquire(1_024).await;
        assert!(started.elapsed().as_millis() >= 850);
        assert_eq!(limiter.snapshot().total_bytes, 2_048);
    }

    #[test]
    fn chunk_size_never_exceeds_the_configured_limit() {
        let limiter = BandwidthLimiter::with_limit_bytes_per_sec(Some(64));
        assert_eq!(limiter.max_chunk_size(128), 64);
        assert_eq!(limiter.max_chunk_size(32), 32);
        assert_eq!(BandwidthLimiter::unlimited().max_chunk_size(128), 128);
    }
}
