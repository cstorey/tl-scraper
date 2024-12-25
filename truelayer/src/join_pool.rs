use std::sync::{Arc, Mutex};

use anyhow::Result;
use futures::{future::BoxFuture, Future, FutureExt};
use tokio::{sync::mpsc, task::JoinSet};
use tracing::{instrument, trace};

#[derive(Clone, Debug, Default)]
struct PoolStats {
    jobs_submitted: usize,
    jobs_started: usize,
    jobs_completed: usize,
}

pub struct JobPool {
    rx: mpsc::UnboundedReceiver<Job>,
    stats: Arc<Mutex<PoolStats>>,
    has_terminated: bool,
    concurrency: usize,
}

struct Job(BoxFuture<'static, Result<()>>);

#[derive(Clone)]
pub struct JobHandle {
    tx: mpsc::UnboundedSender<Job>,
    stats: Arc<Mutex<PoolStats>>,
}

impl JobPool {
    pub fn new(concurrency: usize) -> (Self, JobHandle) {
        let (tx, rx) = mpsc::unbounded_channel();
        let stats = Arc::<Mutex<PoolStats>>::default();
        let pool = JobPool {
            rx,
            concurrency,
            stats: stats.clone(),
            has_terminated: false,
        };
        let handle = JobHandle { tx, stats };
        (pool, handle)
    }

    #[instrument(skip_all)]
    pub async fn run(mut self) -> Result<()> {
        let mut tasks = JoinSet::new();
        loop {
            let stats = self.stats.lock().expect("lock").clone();
            trace!(
                incoming=?!self.has_terminated(),
                tasks=?tasks.len(),
                ?stats.jobs_submitted,
                ?stats.jobs_started,
                ?stats.jobs_completed,
                "Loop"
            );
            if self.has_terminated() && tasks.is_empty() {
                break;
            }

            tokio::select! {
                item = self.next_job(), if tasks.len() < self.concurrency && !self.has_terminated() => {
                    if let Some(Job(fut)) = item? {
                        trace!("Spawning job");
                        self.stats.lock().expect("lock").jobs_started += 1;
                        tasks.spawn(fut);
                    } else {
                        trace!("Channel closed");
                    }
                },
                result = tasks.join_next(), if !tasks.is_empty() => {
                    if let Some(result) = result {
                        self.stats.lock().expect("lock").jobs_completed += 1;
                        trace!("Task exited with: {:?}", result);
                        result??;
                    }
                }
            }
        }
        trace!("Done");
        Ok(())
    }

    async fn next_job(&mut self) -> Result<Option<Job>> {
        if let Some(job) = self.rx.recv().await {
            Ok(Some(job))
        } else {
            self.has_terminated = true;
            Ok(None)
        }
    }

    fn has_terminated(&self) -> bool {
        self.has_terminated
    }
}

impl JobHandle {
    pub fn spawn(&self, fut: impl Future<Output = Result<()>> + Send + 'static) -> Result<()> {
        self.tx
            .send(Job(fut.boxed()))
            .map_err(|_| anyhow::anyhow!("Pool dropped?"))?;
        self.stats.lock().expect("lock").jobs_submitted += 1;

        Ok(())
    }
}
