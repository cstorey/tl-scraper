use std::sync::Arc;

use anyhow::Result;
use futures::{
    channel::mpsc, future::BoxFuture, stream::FusedStream, Future, FutureExt, StreamExt,
};
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore},
    task::JoinSet,
};
use tracing::{instrument, trace};

pub struct JobPool {
    semaphore: Arc<Semaphore>,
    rx: mpsc::UnboundedReceiver<Job>,
}

struct Job(BoxFuture<'static, Result<()>>);

#[derive(Clone)]
pub struct JobHandle {
    tx: mpsc::UnboundedSender<Job>,
}

impl JobPool {
    pub fn new(limit: usize) -> (Self, JobHandle) {
        let semaphore = Arc::new(Semaphore::new(limit));
        let (tx, rx) = mpsc::unbounded();
        (JobPool { rx, semaphore }, JobHandle { tx })
    }

    #[instrument(skip_all)]
    pub async fn run(mut self) -> Result<()> {
        let mut tasks = JoinSet::new();
        loop {
            let available_permits = self.semaphore.available_permits();
            trace!(incoming=?!self.rx.is_terminated(), tasks=?tasks.len(), available_permits, "Loop");
            if self.rx.is_terminated() && tasks.is_empty() {
                break;
            }

            tokio::select! {
                item = self.next_job(), if !self.rx.is_terminated() => {
                    if let Some((Job(fut), permit)) = item? {
                        trace!("Spawning job");
                        tasks.spawn(async move { let res = fut.await; drop(permit); res });
                    } else {
                        trace!("Channel closed");
                    }
                },
                result = tasks.join_next() => {
                    if let Some(result) = result {
                        trace!("Task exited with: {:?}", result);
                        result??;
                    }
                }
            }
        }
        trace!("Done");
        Ok(())
    }

    async fn next_job(&mut self) -> Result<Option<(Job, OwnedSemaphorePermit)>> {
        let permit = self.semaphore.clone().acquire_owned().await?;
        if let Some(job) = self.rx.next().await {
            Ok(Some((job, permit)))
        } else {
            drop(permit);
            Ok(None)
        }
    }
}

impl JobHandle {
    pub fn spawn(&self, fut: impl Future<Output = Result<()>> + Send + 'static) -> Result<()> {
        self.tx
            .unbounded_send(Job(fut.boxed()))
            .map_err(|_| anyhow::anyhow!("Pool dropped?"))?;

        Ok(())
    }
}
