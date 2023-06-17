use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use crate::kv_store::commands::{Command, Sender, Value};

use super::result::*;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RhaiCodeParam {
    code: String,
}

fn max(a: i64, b: i64) -> i64 {
    if a > b {
        a
    } else {
        b
    }
}

fn min(a: i64, b: i64) -> i64 {
    if a < b {
        a
    } else {
        b
    }
}

pub async fn run_rhai_code(
    param: RhaiCodeParam,
    _global_kv_tx: Sender,
    local_kv_tx: Sender,
) -> Result {
    let mut engine = rhai::Engine::new();
    engine.register_fn("max", max);
    engine.register_fn("min", min);

    // Get the keys
    let (resp_tx, resp_rx) = oneshot::channel();
    local_kv_tx
        .send(Command::ListKeys { resp: resp_tx })
        .await?;
    let keys = resp_rx.await??;

    let mut scope = rhai::Scope::new();
    for key in keys {
        let (resp_tx, resp_rx) = oneshot::channel();
        local_kv_tx
            .send(Command::Get {
                key: key.clone(),
                resp: resp_tx,
            })
            .await?;
        let value = resp_rx.await??;
        let value = match value {
            Value::Dynamic(val) => val,
            Value::Array(_) => continue,
        };
        scope.set_or_push(key, value);
    }

    engine.run_with_scope(&mut scope, &param.code)?;

    for (key, _is_constant, value) in scope.iter() {
        let (resp_tx, resp_rx) = oneshot::channel();
        local_kv_tx
            .send(Command::Set {
                key: key.into(),
                value,
                resp: resp_tx,
            })
            .await?;
        resp_rx.await??;
    }

    Ok(FunctionResult::Passed)
}
