#[cfg(feature = "local")]
use anyhow::{anyhow, Result};
#[cfg(feature = "local")]
use rayon::prelude::*;
#[cfg(feature = "local")]
use std::sync::Arc;
#[cfg(feature = "local")]
use tokio::sync::oneshot;

#[cfg(feature = "local")]
#[derive(Clone)]
pub(crate) struct CpuExecutor {
    pool: Option<Arc<rayon::ThreadPool>>,
    #[cfg(test)]
    observer: Option<CpuObserver>,
}

#[cfg(feature = "local")]
impl CpuExecutor {
    pub(crate) fn new() -> Self {
        let thread_count = std::thread::available_parallelism()
            .map(|parallelism| parallelism.get().max(2))
            .unwrap_or(4);

        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(thread_count)
            .thread_name(|index| format!("mxr-semantic-cpu-{index}"))
            .build()
            .map(Arc::new)
            .map_err(|error| {
                tracing::warn!("failed to start semantic cpu executor: {error}");
                error
            })
            .ok();

        Self {
            pool,
            #[cfg(test)]
            observer: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn set_observer(&mut self, observer: CpuObserver) {
        self.observer = Some(observer);
    }

    pub(crate) async fn map<T, R, F>(&self, items: Vec<T>, work: F) -> Result<Vec<R>>
    where
        T: Send + 'static,
        R: Send + 'static,
        F: Fn(T) -> Result<R> + Send + Sync + 'static,
    {
        if items.is_empty() {
            return Ok(Vec::new());
        }

        let Some(pool) = &self.pool else {
            return items
                .into_iter()
                .map(|item| self.run_inline(&work, item))
                .collect();
        };

        let (tx, rx) = oneshot::channel();
        #[cfg(test)]
        let observer = self.observer.clone();
        let work = Arc::new(work);
        let pool = pool.clone();
        pool.spawn_fifo(move || {
            let results = items
                .into_par_iter()
                .map(|item| {
                    #[cfg(test)]
                    let _guard = observer.as_ref().map(CpuObserver::enter);
                    work(item)
                })
                .collect::<Vec<_>>();
            let _ = tx.send(results.into_iter().collect());
        });

        rx.await
            .map_err(|_| anyhow!("semantic cpu executor stopped before finishing work"))?
    }

    fn run_inline<T, R, F>(&self, work: &F, item: T) -> Result<R>
    where
        F: Fn(T) -> Result<R>,
    {
        #[cfg(test)]
        let _guard = self.observer.as_ref().map(CpuObserver::enter);
        work(item)
    }
}

#[cfg(all(test, feature = "local"))]
#[derive(Clone)]
pub(crate) struct CpuObserver {
    current: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    max: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    delay: std::time::Duration,
}

#[cfg(all(test, feature = "local"))]
impl CpuObserver {
    pub(crate) fn new(delay: std::time::Duration) -> Self {
        Self {
            current: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            max: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            delay,
        }
    }

    pub(crate) fn max_concurrency(&self) -> usize {
        self.max.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn enter(&self) -> CpuObserverGuard {
        let current = self
            .current
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;

        loop {
            let max = self.max.load(std::sync::atomic::Ordering::SeqCst);
            if current <= max {
                break;
            }
            if self
                .max
                .compare_exchange(
                    max,
                    current,
                    std::sync::atomic::Ordering::SeqCst,
                    std::sync::atomic::Ordering::SeqCst,
                )
                .is_ok()
            {
                break;
            }
        }

        if !self.delay.is_zero() {
            std::thread::sleep(self.delay);
        }

        CpuObserverGuard {
            observer: self.clone(),
        }
    }
}

#[cfg(all(test, feature = "local"))]
struct CpuObserverGuard {
    observer: CpuObserver,
}

#[cfg(all(test, feature = "local"))]
impl Drop for CpuObserverGuard {
    fn drop(&mut self) {
        self.observer
            .current
            .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}
