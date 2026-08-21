use super::BandwidthSnapshot;
use serde::Serialize;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadProgress {
    pub job_id: String,
    pub downloaded_bytes: i64,
    pub total_bytes: Option<i64>,
    pub speed_bytes_per_sec: Option<i64>,
    pub eta_seconds: Option<i64>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamProgress {
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub speed_bytes_per_sec: Option<u64>,
    pub eta_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LiveProgressEvent {
    pub job_id: String,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub speed_bytes_per_sec: Option<u64>,
    pub eta_seconds: Option<u64>,
    pub bandwidth: BandwidthSnapshot,
}

#[derive(Clone)]
pub struct ProgressBroadcaster {
    sender: broadcast::Sender<LiveProgressEvent>,
}

impl ProgressBroadcaster {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity.max(1));
        Self { sender }
    }

    pub fn publish(&self, event: LiveProgressEvent) {
        let _ = self.sender.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<LiveProgressEvent> {
        self.sender.subscribe()
    }
}

impl Default for ProgressBroadcaster {
    fn default() -> Self {
        Self::new(128)
    }
}

#[derive(Debug, Clone)]
pub struct ProgressSampler {
    interval: Duration,
    started_at: Instant,
    last_emitted_at: Option<Instant>,
    last_emitted_bytes: u64,
}

impl ProgressSampler {
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            started_at: Instant::now(),
            last_emitted_at: None,
            last_emitted_bytes: 0,
        }
    }

    pub fn observe(
        &mut self,
        now: Instant,
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
    ) -> Option<StreamProgress> {
        let should_emit = match self.last_emitted_at {
            Some(last) => now.duration_since(last) >= self.interval,
            None => true,
        };
        should_emit.then(|| self.build_sample(now, downloaded_bytes, total_bytes))
    }

    pub fn finish(
        &mut self,
        now: Instant,
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
    ) -> Option<StreamProgress> {
        if self
            .last_emitted_at
            .is_some_and(|last| now.duration_since(last) < self.interval)
            && self.last_emitted_bytes == downloaded_bytes
        {
            return None;
        }
        Some(self.build_sample(now, downloaded_bytes, total_bytes))
    }

    fn build_sample(
        &mut self,
        now: Instant,
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
    ) -> StreamProgress {
        let elapsed = now.duration_since(self.started_at).as_secs_f64();
        let speed_bytes_per_sec = (elapsed > 0.0)
            .then(|| (downloaded_bytes as f64 / elapsed).round() as u64)
            .filter(|speed| *speed > 0);
        let eta_seconds = total_bytes
            .zip(speed_bytes_per_sec)
            .and_then(|(total, speed)| {
                total
                    .checked_sub(downloaded_bytes)
                    .map(|remaining| remaining / speed)
            });
        self.last_emitted_at = Some(now);
        self.last_emitted_bytes = downloaded_bytes;
        StreamProgress {
            downloaded_bytes,
            total_bytes,
            speed_bytes_per_sec,
            eta_seconds,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ProgressBroadcaster, ProgressSampler};
    use crate::downloader::BandwidthSnapshot;
    use std::time::{Duration, Instant};

    #[test]
    fn throttles_samples_until_the_interval_elapses() {
        let start = Instant::now();
        let mut sampler = ProgressSampler::new(Duration::from_millis(250));
        assert!(sampler.observe(start, 10, Some(100)).is_some());
        assert!(sampler
            .observe(start + Duration::from_millis(100), 20, Some(100))
            .is_none());
        let sample = sampler
            .observe(start + Duration::from_millis(250), 30, Some(100))
            .expect("interval should allow an event");
        assert_eq!(sample.downloaded_bytes, 30);
        assert!(sample.speed_bytes_per_sec.is_some());
    }

    #[test]
    fn finish_emits_a_final_changed_sample_even_inside_the_interval() {
        let start = Instant::now();
        let mut sampler = ProgressSampler::new(Duration::from_secs(1));
        sampler.observe(start, 10, Some(10));
        let sample = sampler
            .finish(start + Duration::from_millis(10), 12, Some(12))
            .expect("final progress should be emitted");
        assert_eq!(sample.downloaded_bytes, 12);
        assert_eq!(sample.total_bytes, Some(12));
    }

    #[tokio::test]
    async fn broadcaster_delivers_live_progress_events() {
        let broadcaster = ProgressBroadcaster::new(4);
        let mut receiver = broadcaster.subscribe();
        broadcaster.publish(super::LiveProgressEvent {
            job_id: "job-1".to_owned(),
            downloaded_bytes: 5,
            total_bytes: Some(10),
            speed_bytes_per_sec: Some(5),
            eta_seconds: Some(1),
            bandwidth: BandwidthSnapshot {
                limit_bytes_per_sec: None,
                current_bytes_per_sec: 5,
                total_bytes: 5,
            },
        });
        let event = receiver.recv().await.expect("event should be delivered");
        assert_eq!(event.job_id, "job-1");
        assert_eq!(event.downloaded_bytes, 5);
    }
}
