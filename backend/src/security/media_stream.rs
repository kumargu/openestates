use std::sync::Arc;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use super::security_tuning;

#[derive(Clone)]
pub struct MediaStreamAdmission {
    slots: Arc<Semaphore>,
}

impl MediaStreamAdmission {
    pub fn from_env() -> Self {
        Self::new(security_tuning().media.stream_concurrency)
    }

    pub fn new(limit: usize) -> Self {
        Self {
            slots: Arc::new(Semaphore::new(limit.max(1))),
        }
    }

    pub fn try_acquire(&self) -> Option<OwnedSemaphorePermit> {
        self.slots.clone().try_acquire_owned().ok()
    }
}
