use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;

#[derive(Clone, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl CancellationToken {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.notify.notify_waiters();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub fn as_atomic(&self) -> &AtomicBool {
        &self.cancelled
    }

    pub async fn cancelled(&self) {
        if self.is_cancelled() {
            return;
        }
        self.notify.notified().await;
    }
}

#[derive(Clone, Default)]
pub struct CancellationRegistry {
    tokens: Arc<Mutex<HashMap<String, CancellationToken>>>,
}

impl CancellationRegistry {
    pub fn register(&self, job_id: String) -> CancellationToken {
        let token = CancellationToken::default();
        self.tokens
            .lock()
            .expect("cancellation registry lock should not be poisoned")
            .insert(job_id, token.clone());
        token
    }

    pub fn cancel(&self, job_id: &str) -> bool {
        let token = self
            .tokens
            .lock()
            .expect("cancellation registry lock should not be poisoned")
            .get(job_id)
            .cloned();
        if let Some(token) = token {
            token.cancel();
            true
        } else {
            false
        }
    }

    pub fn remove(&self, job_id: &str) {
        self.tokens
            .lock()
            .expect("cancellation registry lock should not be poisoned")
            .remove(job_id);
    }
}

#[cfg(test)]
mod tests {
    use super::{CancellationRegistry, CancellationToken};
    use std::time::Duration;

    #[tokio::test]
    async fn cancellation_wakes_waiters_and_sets_atomic_state() {
        let token = CancellationToken::default();
        let waiter = token.clone();
        let task = tokio::spawn(async move {
            waiter.cancelled().await;
            waiter.is_cancelled()
        });
        tokio::time::sleep(Duration::from_millis(1)).await;
        token.cancel();
        assert!(task.await.unwrap());
        assert!(token.as_atomic().load(std::sync::atomic::Ordering::Acquire));
    }

    #[test]
    fn registry_cancels_registered_jobs_and_removes_them() {
        let registry = CancellationRegistry::default();
        let token = registry.register("job-1".to_owned());
        assert!(registry.cancel("job-1"));
        assert!(token.is_cancelled());
        registry.remove("job-1");
        assert!(!registry.cancel("job-1"));
    }
}
