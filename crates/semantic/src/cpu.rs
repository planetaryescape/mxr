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
}

#[cfg(feature = "local")]
impl CpuExecutor {
    pub(crate) fn new() -> Self {
        let thread_count =
            std::thread::available_parallelism().map_or(4, |parallelism| parallelism.get().max(2));

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

        Self { pool }
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
        let work = Arc::new(work);
        let pool = pool.clone();
        pool.spawn_fifo(move || {
            let results = items
                .into_par_iter()
                .map(|item| work(item))
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
        work(item)
    }
}
