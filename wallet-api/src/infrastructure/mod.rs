pub mod cache;
pub mod task_queue;

pub mod inner_event;
pub mod log;
pub mod mqtt;
pub mod process_fee_tx;
pub mod process_unconfirm_msg;
pub mod process_withdraw_tx;
pub mod swap_client;
pub mod process_collect_tx;

pub use cache::*;
use chrono::{DateTime, NaiveDateTime, Utc};

// time 转换, 默认返回 1970-01-01
pub fn parse_utc_datetime(s: &str) -> DateTime<Utc> {
    match NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        Ok(naive) => DateTime::from_naive_utc_and_offset(naive, Utc),
        Err(_) => DateTime::<Utc>::default(),
    }
}

pub fn parse_utc_with_error(s: &str) -> Result<DateTime<Utc>, crate::error::service::ServiceError> {
    match NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        Ok(naive) => Ok(DateTime::from_naive_utc_and_offset(naive, Utc)),
        Err(e) => {
            let e = crate::error::service::ServiceError::Parameter(format!(
                "convert time error: {}",
                e
            ));
            Err(e)
        }
    }
}
