use tokio::{sync::mpsc, task::JoinHandle};

#[derive(Debug)]
pub(crate) struct ProcessWithdrawTxHandle {
    tx: mpsc::Sender<String>,
    handle: Option<JoinHandle<Result<(), anyhow::Error>>>,
}

impl ProcessWithdrawTxHandle {
    pub(crate) async fn new() -> Self {
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        let mut n = Self { tx: shutdown_tx, handle: None };
        let mut tx = ProcessWithdrawTx::new(shutdown_rx);
        let handle = tokio::spawn(async move { tx.run().await });
        n.handle = Some(handle);
        n
    }

    pub(crate) async fn submit_tx(&mut self, tx: String) -> Result<(), anyhow::Error> {
        let _ = self.tx.send("tx".to_string());
        Ok(())
    }

    pub async fn close(&mut self) -> Result<(), anyhow::Error> {
        let _ = self.tx.send("close".to_string());
        if let Some(handle) = self.handle.take() {
            handle.await?; // JoinHandle::await 返回 Result<T, JoinError>
        }
        Ok(())
    }
}

pub(crate) struct ProcessWithdrawTx {
    rx: mpsc::Receiver<String>,
}

impl ProcessWithdrawTx {
    pub(crate) fn new(rx: mpsc::Receiver<String>) -> Self {
        Self { rx }
    }

    pub(crate) async fn run(&mut self) -> Result<(), anyhow::Error> {
        let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            tokio::select! {
                msg = self.rx.recv() => {
                    if msg == Some("close".to_string()) {
                        break;
                    } else {
                        Self::process_withdraw_tx().await?;
                        iv.reset();
                    }
                }
                _ = iv.tick() => {
                    Self::process_withdraw_tx().await?;
                }
            }
        }
        Ok(())
    }

    async fn process_withdraw_tx() -> Result<(), anyhow::Error> {
        Ok(())
    }
}
