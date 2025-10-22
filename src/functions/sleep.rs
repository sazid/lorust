use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::time;

use crate::kv_store::commands::Sender;

use super::result::*;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SleepParam {
    duration: String,
}

pub async fn sleep(param: SleepParam, timeout: Option<Duration>, _kv_tx: Sender) -> FunctionResult {
    println!("Sleeping for {} secs", param.duration);
    let duration = param.duration.parse::<u64>()?;
    let sleep_time = match timeout {
        Some(t) => std::cmp::min(Duration::from_secs(duration), t),
        None => Duration::from_secs(duration),
    };
    time::sleep(sleep_time).await;

    Ok(FunctionStatus::Passed)
}
