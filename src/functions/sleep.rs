use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::time;

use super::result::*;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SleepParam {
    duration: u64,
}

pub async fn sleep(param: SleepParam) -> Result {
    println!("Sleeping for {} secs", param.duration);
    time::sleep(Duration::from_secs(param.duration)).await;

    Ok(FunctionResult::Passed)
}
