use std::borrow::Cow;
use std::collections::BTreeMap;
use std::time::Duration;

use regex::Regex;

use rhai::Dynamic;
use tokio::time::Instant;

use crate::flow::{Flow, Function};
use crate::kv_store::{commands::Sender, store::new as kv_store_new};

use super::http_request;
use super::load_gen;
use super::result::*;
use super::rhai_code;
use super::sleep;

pub async fn run_flow(flow: Flow, kv_tx: Sender) -> FunctionResult {
    run_loadgen(flow.functions, kv_tx.clone()).await?;

    Ok(FunctionStatus::Passed)
}

pub async fn run_loadgen(functions: Vec<Function>, kv_tx: Sender) -> FunctionResult {
    for (index, function) in functions.into_iter().enumerate() {
        println!("--- Running function #{} ---", index + 1);
        match function {
            Function::LoadGen(param) => load_gen::load_gen(param.clone(), kv_tx.clone()).await?,
            _ => panic!("top level function must be loadgen"),
        };
    }

    Ok(FunctionStatus::Passed)
}

async fn interpolate_variables(input: &str, local_kv_tx: Sender) -> Result<Cow<'_, str>> {
    let mut map: BTreeMap<&str, Dynamic> = BTreeMap::new();

    let re = Regex::new(r"%\|(.+?)\|%").unwrap();

    // Fill the values map with the key names.
    for key in re.find_iter(input) {
        let key = key.as_str();
        let key = &key[2..key.len() - 2];

        let value = rhai_code::eval_rhai_code(key, local_kv_tx.clone()).await?;

        map.insert(key, value);
    }

    // Replace the key names with their corresponding string values
    let replaced = re.replace_all(input, |caps: &regex::Captures| {
        let key = &caps[1];

        match map.get(key).cloned() {
            Some(value) => value.to_string(),
            None => format!("NO_SUCH_VARIABLE:{key}"),
        }
    });

    Ok(replaced)
}

pub async fn run_functions(
    functions: Vec<Function>,
    global_kv_tx: Sender,
    timeout: u64,
) -> FunctionResult {
    // TODO: Instead of defining something like this, there should be proper
    // scoping mechanisms with scope names that can be referred from inside
    // functions. Maybe a graph of scopes that child scopes can refer back to?
    let (local_kv_handle, local_kv_tx) = kv_store_new().await;

    let end_time = Instant::now() + Duration::from_secs(timeout);

    // Perform variable interpolation and execute the Functions.
    for (_, function) in functions.into_iter().enumerate() {
        if Instant::now() >= end_time {
            break;
        }

        // 1. Convert the Function a string.
        let function: String = serde_json::to_string(&function)?;

        // 2. Perform variable (string) interpolation and insert variable values.
        let function: Cow<'_, str> = interpolate_variables(&function, local_kv_tx.clone()).await?;

        // 3. Convert the interpolated string back to a Function that can be executed.
        let function: Function = serde_json::from_str(&function)?;

        // 4. Execute the Function.
        let remaining_time = Some(end_time - Instant::now());
        match function {
            Function::HttpRequest(param) => {
                http_request::make_request(
                    param.clone(),
                    remaining_time,
                    global_kv_tx.clone(),
                    local_kv_tx.clone(),
                )
                .await?
            }
            Function::Sleep(param) => {
                sleep::sleep(param.clone(), remaining_time, global_kv_tx.clone()).await?
            }
            Function::RunRhaiCode(param) => {
                rhai_code::run_rhai_code(param, global_kv_tx.clone(), local_kv_tx.clone()).await?
            }
            Function::LoadGen(_) => panic!("load gen function cannot be nested"),
        };
    }

    drop(local_kv_tx);
    local_kv_handle.await?;

    Ok(FunctionStatus::Passed)
}
