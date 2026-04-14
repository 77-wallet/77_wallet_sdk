use dashmap::DashMap;
use once_cell::sync::Lazy;
use std::{future::Future, sync::Arc};
use tokio::{sync::Mutex, sync::Notify};

struct FlightState<T> {
    running: bool,
    result: Option<Result<Arc<T>, String>>,
}

struct SharedFlight<T> {
    state: Mutex<FlightState<T>>,
    notify: Notify,
}

impl<T> SharedFlight<T> {
    fn new() -> Self {
        Self {
            state: Mutex::new(FlightState { running: false, result: None }),
            notify: Notify::new(),
        }
    }
}

static FLIGHTS: Lazy<DashMap<String, Arc<SharedFlight<crate::response_vo::standard_wallet::account::BalanceInfo>>>> =
    Lazy::new(DashMap::new);

fn get_flight(key: &str) -> Arc<SharedFlight<crate::response_vo::standard_wallet::account::BalanceInfo>> {
    FLIGHTS
        .entry(key.to_string())
        .or_insert_with(|| Arc::new(SharedFlight::new()))
        .clone()
}

fn to_result(
    value: Result<Arc<crate::response_vo::standard_wallet::account::BalanceInfo>, String>,
) -> Result<crate::response_vo::standard_wallet::account::BalanceInfo, String> {
    value.map(|v| v.as_ref().clone())
}

pub async fn execute_shared<F, Fut>(
    key: &str,
    query_fn: F,
) -> Result<crate::response_vo::standard_wallet::account::BalanceInfo, String>
where
    F: FnOnce() -> Fut,
    Fut: Future<
        Output = Result<crate::response_vo::standard_wallet::account::BalanceInfo, String>,
    >,
{
    let flight = get_flight(key);

    loop {
        let mut state = flight.state.lock().await;
        if let Some(result) = state.result.clone() {
            return to_result(result);
        }

        if !state.running {
            state.running = true;
            drop(state);

            let result = query_fn().await.map(Arc::new);
            let mut state = flight.state.lock().await;
            state.running = false;
            state.result = Some(result.clone());
            flight.notify.notify_waiters();
            return to_result(result);
        }

        let notified = flight.notify.notified();
        drop(state);
        notified.await;
    }
}

#[cfg(test)]
mod tests {
    use super::execute_shared;
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
                execute_shared(&key, || async move {
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
