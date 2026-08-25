use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use tokio::runtime::Handle;
use tokio::sync::Semaphore;

use super::security_tuning;

#[derive(Clone)]
pub struct ExecutionLanes {
    internal: Handle,
    customer_compute: Handle,
    customer_compute_slots: Arc<Semaphore>,
    customer_compute_queue_timeout: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CustomerComputeError {
    Overloaded,
    Cancelled,
}

impl ExecutionLanes {
    pub fn new(internal: Handle, customer_compute: Handle, customer_compute_limit: usize) -> Self {
        Self::new_with_queue_timeout(
            internal,
            customer_compute,
            customer_compute_limit,
            Duration::from_millis(security_tuning().runtime.customer_compute_queue_timeout_ms),
        )
    }

    fn new_with_queue_timeout(
        internal: Handle,
        customer_compute: Handle,
        customer_compute_limit: usize,
        customer_compute_queue_timeout: Duration,
    ) -> Self {
        Self {
            internal,
            customer_compute,
            customer_compute_slots: Arc::new(Semaphore::new(customer_compute_limit.max(1))),
            customer_compute_queue_timeout,
        }
    }

    /// Tests and embedded callers can retain their current-runtime behavior.
    /// Production passes distinct handles from `main`.
    pub fn current() -> Self {
        let handle = Handle::current();
        Self::new(
            handle.clone(),
            handle,
            security_tuning().runtime.customer_compute_limit,
        )
    }

    pub fn spawn_internal<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.internal.spawn(future);
    }

    pub fn spawn_internal_blocking<F>(&self, work: F)
    where
        F: FnOnce() + Send + 'static,
    {
        self.internal.spawn_blocking(work);
    }

    pub async fn run_internal<F, T>(&self, future: F) -> Result<T, tokio::task::JoinError>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        self.internal.spawn(future).await
    }

    /// Runs synchronous customer work away from HTTP coordination threads.
    /// Admission waits only for the configured short window. Route-class
    /// admission bounds the number of waiters, so Tokio's blocking pool never
    /// receives an unbounded queue.
    pub async fn run_customer_compute<F, T>(&self, work: F) -> Result<T, CustomerComputeError>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let permit = tokio::time::timeout(
            self.customer_compute_queue_timeout,
            self.customer_compute_slots.clone().acquire_owned(),
        )
        .await
        .map_err(|_| CustomerComputeError::Overloaded)?
        .map_err(|_| CustomerComputeError::Cancelled)?;
        self.customer_compute
            .spawn_blocking(move || {
                let _permit = permit;
                work()
            })
            .await
            .map_err(|_| CustomerComputeError::Cancelled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn customer_compute_sheds_when_all_slots_are_in_use() {
        let lanes = ExecutionLanes::new_with_queue_timeout(
            Handle::current(),
            Handle::current(),
            1,
            Duration::from_millis(5),
        );
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let task_lanes = lanes.clone();
        let task_barrier = barrier.clone();
        let running = tokio::spawn(async move {
            task_lanes
                .run_customer_compute(move || {
                    task_barrier.wait();
                    std::thread::sleep(std::time::Duration::from_millis(30));
                })
                .await
        });
        barrier.wait();

        assert_eq!(
            lanes.run_customer_compute(|| ()).await,
            Err(CustomerComputeError::Overloaded)
        );
        assert_eq!(running.await.unwrap(), Ok(()));
    }
}
