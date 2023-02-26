use anyhow::Result;
use futures::{
    channel::mpsc, future::BoxFuture, stream::FusedStream, Future, FutureExt, StreamExt,
};
use tokio::task::JoinSet;
use tracing::{instrument, trace};

pub struct JobPool {
    limit: usize,
    rx: mpsc::UnboundedReceiver<Job>,
}

struct Job(BoxFuture<'static, Result<()>>);

#[derive(Clone)]
pub struct JobHandle {
    tx: mpsc::UnboundedSender<Job>,
}

impl JobPool {
    pub fn new(limit: usize) -> (Self, JobHandle) {
        let (tx, rx) = mpsc::unbounded();
        (JobPool { rx, limit }, JobHandle { tx })
    }

    #[instrument(skip_all)]
    pub async fn run(mut self) -> Result<()> {
        let mut tasks = JoinSet::new();
        loop {
            let has_capacity = tasks.len() < self.limit;
            trace!(incoming=?self.rx.is_terminated(), tasks=?tasks.len(), ?has_capacity, "Loop");
            if self.rx.is_terminated() && tasks.is_empty() {
                break;
            }
            tokio::select! {
                item = self.rx.next(), if has_capacity && !self.rx.is_terminated() => {
                    if let Some(Job(fut)) = item {
                        trace!("Spawning job");
                        tasks.spawn(fut);
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
}

impl JobHandle {
    pub fn spawn(&self, fut: impl Future<Output = Result<()>> + Send + 'static) -> Result<()> {
        self.tx
            .unbounded_send(Job(fut.boxed()))
            .map_err(|_| anyhow::anyhow!("Pool dropped?"))?;

        Ok(())
    }
}
