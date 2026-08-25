use std::{collections::HashMap, sync::Mutex, time::Duration};

use tokio::{sync::Notify, time::Instant};

const DEFAULT_FOREGROUND_LEASE: Duration = Duration::from_secs(120);

#[derive(Debug, Default)]
struct RecoveryState {
    globally_paused: bool,
    foreground: HashMap<String, Instant>,
}

/// Central admission policy for API-wallet recovery work.
///
/// Priority order:
/// 1. a global pause, used by destructive foreground operations;
/// 2. recovery belonging to newly imported wallets;
/// 3. unrelated background recovery.
#[derive(Debug)]
pub struct RecoveryCoordinator {
    state: Mutex<RecoveryState>,
    changed: Notify,
    foreground_lease: Duration,
}

impl Default for RecoveryCoordinator {
    fn default() -> Self {
        Self::new(DEFAULT_FOREGROUND_LEASE)
    }
}

impl RecoveryCoordinator {
    fn new(foreground_lease: Duration) -> Self {
        Self {
            state: Mutex::new(RecoveryState::default()),
            changed: Notify::new(),
            foreground_lease,
        }
    }

    pub fn set_globally_paused(&self, paused: bool) {
        self.state.lock().expect("recovery coordinator mutex poisoned").globally_paused = paused;
        self.changed.notify_waiters();
    }

    pub fn is_globally_paused(&self) -> bool {
        self.state.lock().expect("recovery coordinator mutex poisoned").globally_paused
    }

    /// Registers or renews a wallet's foreground import lease.
    pub fn prioritize_wallet(&self, uid: &str) {
        self.state
            .lock()
            .expect("recovery coordinator mutex poisoned")
            .foreground
            .insert(uid.to_owned(), Instant::now() + self.foreground_lease);
        self.changed.notify_waiters();
    }

    /// Waits for admission at a safe task/page boundary.
    ///
    /// `wallet_uid = None` represents foreground work that is affected only by
    /// the destructive global pause. Recovery work supplies its wallet UID and
    /// additionally yields to foreground-wallet leases.
    pub async fn wait_for_turn(&self, wallet_uid: Option<&str>) {
        loop {
            let notified = self.changed.notified();
            tokio::pin!(notified);

            let wait_until = {
                let now = Instant::now();
                let mut state = self.state.lock().expect("recovery coordinator mutex poisoned");
                state.foreground.retain(|_, expires_at| *expires_at > now);

                if state.globally_paused {
                    None
                } else if wallet_uid.is_none()
                    || state.foreground.is_empty()
                    || wallet_uid.is_some_and(|uid| state.foreground.contains_key(uid))
                {
                    return;
                } else {
                    Some(state.foreground.values().copied().min().expect("foreground is not empty"))
                }
            };

            match wait_until {
                Some(deadline) => {
                    tokio::select! {
                        _ = &mut notified => {}
                        _ = tokio::time::sleep_until(deadline) => {}
                    }
                }
                None => notified.await,
            }
        }
    }

    #[cfg(test)]
    fn finish_foreground_wallet(&self, uid: &str) {
        self.state.lock().expect("recovery coordinator mutex poisoned").foreground.remove(uid);
        self.changed.notify_waiters();
    }
}

#[cfg(test)]
mod recovery_coordinator_tests {
    use std::{sync::Arc, time::Duration};

    use super::RecoveryCoordinator;

    #[tokio::test]
    async fn recovery_coordinator_allows_foreground_wallet() {
        let coordinator = RecoveryCoordinator::new(Duration::from_secs(1));
        coordinator.prioritize_wallet("new-wallet");

        tokio::time::timeout(
            Duration::from_millis(20),
            coordinator.wait_for_turn(Some("new-wallet")),
        )
        .await
        .expect("foreground wallet should not wait");
    }

    #[tokio::test]
    async fn recovery_coordinator_waits_for_unrelated_wallet() {
        let coordinator = Arc::new(RecoveryCoordinator::new(Duration::from_secs(1)));
        coordinator.prioritize_wallet("new-wallet");

        let waiting = {
            let coordinator = Arc::clone(&coordinator);
            tokio::spawn(async move { coordinator.wait_for_turn(Some("old-wallet")).await })
        };
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(!waiting.is_finished());

        coordinator.finish_foreground_wallet("new-wallet");
        tokio::time::timeout(Duration::from_millis(100), waiting)
            .await
            .expect("old wallet should resume")
            .expect("wait task should succeed");
    }

    #[tokio::test]
    async fn recovery_coordinator_global_pause_blocks_foreground_wallet() {
        let coordinator = Arc::new(RecoveryCoordinator::new(Duration::from_secs(1)));
        coordinator.prioritize_wallet("new-wallet");
        coordinator.set_globally_paused(true);

        let waiting = {
            let coordinator = Arc::clone(&coordinator);
            tokio::spawn(async move { coordinator.wait_for_turn(Some("new-wallet")).await })
        };
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(!waiting.is_finished());

        coordinator.set_globally_paused(false);
        tokio::time::timeout(Duration::from_millis(100), waiting)
            .await
            .expect("foreground wallet should resume after global pause")
            .expect("wait task should succeed");
    }

    #[tokio::test]
    async fn recovery_coordinator_allows_multiple_foreground_wallets() {
        let coordinator = RecoveryCoordinator::new(Duration::from_secs(1));
        coordinator.prioritize_wallet("recharge-wallet");
        coordinator.prioritize_wallet("withdraw-wallet");

        for uid in ["recharge-wallet", "withdraw-wallet"] {
            tokio::time::timeout(Duration::from_millis(20), coordinator.wait_for_turn(Some(uid)))
                .await
                .expect("each foreground wallet should proceed");
        }
    }

    #[tokio::test]
    async fn recovery_coordinator_expires_stale_foreground_lease() {
        let coordinator = RecoveryCoordinator::new(Duration::from_millis(20));
        coordinator.prioritize_wallet("abandoned-import");

        tokio::time::timeout(
            Duration::from_millis(100),
            coordinator.wait_for_turn(Some("old-wallet")),
        )
        .await
        .expect("stale foreground lease should expire");
    }
}
