use serde::{Deserialize, Serialize};

use crate::kv_store::commands::Sender;

use super::result::*;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RhaiCodeParam {
    code: String,
}

pub async fn run_rhai_code(
    param: RhaiCodeParam,
    global_kv_tx: Sender,
    local_kv_tx: Sender,
) -> Result {
    rhai::run(&param.code)?;

    Ok(FunctionResult::Passed)
}
