use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::interval;

pub struct CaptureScheduler {
    interval_ms: u64,
    stop_tx: Option<mpsc::Sender<()>>,
}

impl CaptureScheduler {
    pub fn new(interval_ms: u64) -> Self {
        Self {
            interval_ms,
            stop_tx: None,
        }
    }

    pub fn set_interval(&mut self, interval_ms: u64) {
        self.interval_ms = interval_ms;
    }

    pub async fn start<F>(&mut self, mut callback: F)
    where
        F: FnMut() + Send + 'static,
    {
        let (stop_tx, mut stop_rx) = mpsc::channel::<()>(1);
        self.stop_tx = Some(stop_tx);

        let interval_ms = self.interval_ms;

        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_millis(interval_ms));

            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        callback();
                    }
                    _ = stop_rx.recv() => {
                        break;
                    }
                }
            }
        });
    }

    pub async fn stop(&mut self) {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(()).await;
        }
    }
}
