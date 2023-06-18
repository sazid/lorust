use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::time;

use crate::kv_store::commands::Sender;

use super::result::*;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SleepParam {
    duration: String,
}

pub async fn sleep(param: SleepParam, _kv_tx: Sender) -> FunctionResult {
    println!("Sleeping for {} secs", param.duration);
    let duration = param.duration.parse::<u64>()?;
    time::sleep(Duration::from_secs(duration)).await;

    Ok(FunctionStatus::Passed)
}
