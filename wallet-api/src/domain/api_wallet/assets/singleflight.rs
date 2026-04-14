use dashmap::DashMap;
use once_cell::sync::Lazy;
use std::{future::Future, sync::Arc};
use tokio::sync::{Mutex, Notify};

struct Entry<T> {
    running: bool,
    result: Option<Result<Arc<T>, String>>,
}

struct Flight<T> {
    state: Mutex<Entry<T>>,
    notify: Notify,
}

impl<T> Flight<T> {
    fn new() -> Self {
        Self {
            state: Mutex::new(Entry { running: false, result: None }),
            notify: Notify::new(),
        }
    }
}

pub struct SingleFlight<T> {
    flights: DashMap<String, Arc<Flight<T>>>,
}

impl<T> Default for SingleFlight<T> {
    fn default() -> Self {
        Self { flights: DashMap::new() }
    }
}

impl<T> SingleFlight<T>
where
    T: Clone + Send + Sync + 'static,
{
    fn flight(&self, key: &str) -> Arc<Flight<T>> {
        self.flights
            .entry(key.to_string())
            .or_insert_with(|| Arc::new(Flight::new()))
            .clone()
    }

    pub async fn call<F, Fut>(&self, key: &str, query_fn: F) -> Result<T, String>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, String>>,
    {
        let flight = self.flight(key);

        loop {
            let mut state = flight.state.lock().await;
            if let Some(result) = state.result.clone() {
                return result.map(|value| value.as_ref().clone());
            }

            if !state.running {
                state.running = true;
                drop(state);

                let result = query_fn().await.map(Arc::new);
                let mut state = flight.state.lock().await;
                state.running = false;
                state.result = Some(result.clone());
                flight.notify.notify_waiters();
                return result.map(|value| value.as_ref().clone());
            }

            let notified = flight.notify.notified();
            drop(state);
            notified.await;
        }
    }
}

static BALANCE_INFO_SINGLE_FLIGHT: Lazy<
    SingleFlight<crate::response_vo::standard_wallet::account::BalanceInfo>,
> = Lazy::new(SingleFlight::default);

pub async fn call_balance_info<F, Fut>(
    key: &str,
    query_fn: F,
) -> Result<crate::response_vo::standard_wallet::account::BalanceInfo, String>
where
    F: FnOnce() -> Fut,
    Fut: Future<
        Output = Result<crate::response_vo::standard_wallet::account::BalanceInfo, String>,
    >,
{
    BALANCE_INFO_SINGLE_FLIGHT.call(key, query_fn).await
}

#[cfg(test)]
mod tests {
    use super::call_balance_info;
    use crate::response_vo::standard_wallet::account::BalanceInfo;
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::{SystemTime, UNIX_EPOCH},
    };
    use tokio::time::{Duration, sleep};

    fn unique_key(prefix: &str) -> String {
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        format!("{prefix}-{ts}")
    }

    #[tokio::test]
    async fn shared_flight_only_runs_once_for_concurrent_calls() {
        let key = unique_key("shared-flight");
        let hit_count = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();

        for _ in 0..32 {
            let key = key.clone();
            let hit_count = hit_count.clone();
            tasks.push(tokio::spawn(async move {
                call_balance_info(&key, || async move {
                    hit_count.fetch_add(1, Ordering::SeqCst);
                    sleep(Duration::from_millis(20)).await;
                    Ok(BalanceInfo {
                        amount: 1.0,
                        currency: "USD".to_string(),
                        unit_price: None,
                        fiat_value: Some(1.0),
                    })
                })
                .await
            }));
        }

        for task in tasks {
            let res = task.await.expect("join ok");
            assert!(res.is_ok());
        }

        assert_eq!(hit_count.load(Ordering::SeqCst), 1);
    }
}
