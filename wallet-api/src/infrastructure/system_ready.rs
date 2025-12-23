use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Notify;

static SYSTEM_READY: AtomicBool = AtomicBool::new(false);
static READY_NOTIFY: Lazy<Notify> = Lazy::new(Notify::new);

pub async fn wait_system_ready() {
    if SYSTEM_READY.load(Ordering::Acquire) {
        tracing::info!("system already ready");
        return;
    }
    tracing::info!("waiting system ready...");
    READY_NOTIFY.notified().await;
    tracing::info!("system ready notified");
}

pub fn mark_system_ready() {
    if !SYSTEM_READY.swap(true, Ordering::Release) {
        READY_NOTIFY.notify_waiters();
    }
    tracing::info!("system marked ready");
}

pub fn is_system_ready() -> bool {
    SYSTEM_READY.load(Ordering::Acquire)
}
