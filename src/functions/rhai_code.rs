use rhai::packages::Package;
use rhai::Dynamic;
use rhai_rand::RandomPackage;
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

/// Initializes the scope with all the variables from the kv store and registers
/// some necessary functions to the engine.
async fn init_engine_and_scope(
    local_kv_tx: &tokio::sync::mpsc::Sender<Command>,
) -> std::result::Result<
    (rhai::Engine, rhai::Scope<'static>),
    Box<dyn std::error::Error + Send + Sync>,
> {
    let mut engine = rhai::Engine::new();
    engine.register_fn("max", max);
    engine.register_fn("min", min);
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
        let mut value = match value {
            Value::Dynamic(val) => val,
            Value::Array(val) => Dynamic::from_array(val),
        };

        // If it's a json string, we try to convert it to a rhai::Map,
        // otherwise just store the plain string.
        if value.is::<String>() {
            value = match engine.parse_json(value.take().cast::<String>(), true) {
                Ok(map) => Dynamic::from_map(map),
                _ => value,
            };
        }
        scope.set_or_push(key, value);
    }
    let random = RandomPackage::new();
    random.register_into_engine(&mut engine);
    Ok((engine, scope))
}

pub async fn run_rhai_code(
    param: RhaiCodeParam,
    _global_kv_tx: Sender,
    local_kv_tx: Sender,
) -> FunctionResult {
    let (engine, mut scope) = init_engine_and_scope(&local_kv_tx).await?;

    // Run the code.
    engine.run_with_scope(&mut scope, &param.code)?;

    // Read all the variables and store/overwrite them in the store.
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

    Ok(FunctionStatus::Passed)
}

pub async fn eval_rhai_code(code: &str, local_kv_tx: Sender) -> Result<Dynamic> {
    let (engine, mut scope) = init_engine_and_scope(&local_kv_tx).await?;

    // Run the code
    Ok(engine.eval_with_scope::<Dynamic>(&mut scope, code)?)
}
