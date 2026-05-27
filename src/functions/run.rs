use std::borrow::Cow;
use std::collections::BTreeMap;
use std::time::Duration;

use regex::Regex;

use serde_json::Value as JsonValue;
use tokio::time::Instant;

use crate::flow::{Flow, Function};
use crate::kv_store::{commands::Sender, store::new as kv_store_new};

use super::http_request;
use super::load_gen;
use super::python_code;
use super::result::*;
use super::sleep;

#[derive(Debug, Clone)]
pub struct TaskContext {
    pub run_id: String,
    pub worker_id: String,
    pub task_id: u64,
}

pub async fn run_flow(flow: Flow, kv_tx: Sender) -> FunctionResult {
    run_loadgen(flow.functions, kv_tx.clone()).await
}

pub async fn run_loadgen(functions: Vec<Function>, kv_tx: Sender) -> FunctionResult {
    let mut final_status = FunctionStatus::Passed;

    for (index, function) in functions.into_iter().enumerate() {
        println!("--- Running function #{} ---", index + 1);
        match function {
            Function::LoadGen(param) => {
                match load_gen::load_gen(param.clone(), kv_tx.clone()).await? {
                    FunctionStatus::Passed => {}
                    FunctionStatus::Failed => final_status = FunctionStatus::Failed,
                }
            }
            _ => panic!("top level function must be loadgen"),
        };
    }

    Ok(final_status)
}

async fn interpolate_variables(input: &str, local_kv_tx: Sender) -> Result<Cow<'_, str>> {
    let mut map: BTreeMap<&str, JsonValue> = BTreeMap::new();

    let re = Regex::new(r"%\|(.+?)\|%").unwrap();

    // Fill the values map with the key names.
    for key in re.find_iter(input) {
        let key = key.as_str();
        let key = &key[2..key.len() - 2];

        let value = python_code::eval_python_expression(key, local_kv_tx.clone()).await?;

        map.insert(key, value);
    }

    // Replace the key names with their corresponding string values
    let replaced = re.replace_all(input, |caps: &regex::Captures| {
        let key = &caps[1];

        match map.get(key).cloned() {
            Some(JsonValue::String(value)) => value,
            Some(JsonValue::Null) => String::new(),
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
    task_context: TaskContext,
) -> FunctionResult {
    // TODO: Instead of defining something like this, there should be proper
    // scoping mechanisms with scope names that can be referred from inside
    // functions. Maybe a graph of scopes that child scopes can refer back to?
    let (local_kv_handle, local_kv_tx) = kv_store_new().await;
    let should_collect_metrics = http_request::should_collect_metrics(&global_kv_tx).await?;
    let http_client = http_request::new_client(should_collect_metrics)?;
    let mut http_metrics = Vec::new();
    let mut http_sequence = 0;

    let end_time = Instant::now() + Duration::from_secs(timeout);
    let mut final_status = FunctionStatus::Passed;

    // Perform variable interpolation and execute the Functions.
    for (index, function) in functions.into_iter().enumerate() {
        if Instant::now() >= end_time {
            break;
        }

        let exec_result: FunctionResult = async {
            // 1. Convert the Function to a string.
            let function_str = serde_json::to_string(&function)?;

            // 2. Perform variable (string) interpolation and insert variable values.
            let interpolated = interpolate_variables(&function_str, local_kv_tx.clone()).await?;

            // 3. Convert the interpolated string back to a Function that can be executed.
            let executable_function: Function = serde_json::from_str(interpolated.as_ref())?;

            // 4. Execute the Function.
            let remaining_time = end_time.checked_duration_since(Instant::now());
            match executable_function {
                Function::HttpRequest(param) => {
                    let metric_identity = if should_collect_metrics {
                        http_sequence += 1;
                        Some(http_request::HttpMetricIdentity {
                            run_id: task_context.run_id.clone(),
                            worker_id: task_context.worker_id.clone(),
                            task_id: task_context.task_id,
                            sequence: http_sequence,
                        })
                    } else {
                        None
                    };
                    let metrics = should_collect_metrics.then_some(&mut http_metrics);
                    http_request::make_request(
                        param,
                        remaining_time,
                        http_client.clone(),
                        metric_identity,
                        metrics,
                        local_kv_tx.clone(),
                    )
                    .await
                }
                Function::Sleep(param) => {
                    sleep::sleep(param, remaining_time, global_kv_tx.clone()).await
                }
                Function::RunPythonCode(param) => {
                    python_code::run_python_code(param, global_kv_tx.clone(), local_kv_tx.clone())
                        .await
                }
                Function::LoadGen(_) => panic!("load gen function cannot be nested"),
            }
        }
        .await;

        match exec_result {
            Ok(FunctionStatus::Passed) => {}
            Ok(FunctionStatus::Failed) => {
                final_status = FunctionStatus::Failed;
                break;
            }
            Err(err) => {
                eprintln!(
                    "Function #{} execution failed with error: {}",
                    index + 1,
                    err
                );
                final_status = FunctionStatus::Failed;
                break;
            }
        }
    }

    if should_collect_metrics {
        if let Err(err) = http_request::append_metrics(&global_kv_tx, http_metrics).await {
            eprintln!("Failed to flush task HTTP metrics: {}", err);
            final_status = FunctionStatus::Failed;
        }
    }

    drop(local_kv_tx);
    if let Err(err) = local_kv_handle.await {
        eprintln!("Local KV store task ended with error: {}", err);
        final_status = FunctionStatus::Failed;
    }

    Ok(final_status)
}
