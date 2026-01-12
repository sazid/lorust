use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};

use crate::{
    flow::Function,
    functions::{http_request::HttpMetric, python_code, run::run_functions},
    kv_store::commands::{Command, Sender},
};

use tokio::{
    sync::oneshot,
    time::{sleep, Duration},
};

use super::result::*;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LoadGenParam {
    spawn_rate: String,

    timeout: u64,

    #[serde(default)]
    max_tasks: Option<u64>,

    functions_to_execute: Vec<Function>,
}

fn eval_task_count(
    interpreter: &python_code::PythonInterpreter,
    expression: &str,
    tick: i64,
) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
    let mut context = BTreeMap::new();
    context.insert("TICK".to_string(), json!(tick));

    let result = python_code::eval_expression_with_context(interpreter, expression, &context)?;
    match result {
        JsonValue::Number(num) => {
            if let Some(value) = num.as_i64() {
                Ok(value)
            } else if let Some(value) = num.as_u64() {
                Ok(value as i64)
            } else {
                Err(format!("expression '{expression}' produced non-integer value").into())
            }
        }
        other => {
            Err(format!("expression '{expression}' produced non-numeric value: {other}").into())
        }
    }
}

pub async fn load_gen(param: LoadGenParam, kv_tx: Sender) -> FunctionResult {
    println!("Running load generator with the config:");
    let mut config_display = param.clone();
    config_display.functions_to_execute = Vec::new();
    println!("{:?}", config_display);

    let eval_interpreter = python_code::PythonInterpreter::new()?;

    let metrics = json!([]);
    let (resp_tx, resp_rx) = oneshot::channel();
    kv_tx
        .send(Command::Set {
            key: "load_gen_metrics".into(),
            value: metrics,
            resp: resp_tx,
        })
        .await?;
    let _ = resp_rx.await?;

    let mut tasks = Vec::new();

    let mut tick = 0;
    let num_users = match param.max_tasks {
        Some(value) if value > 0 => value,
        Some(_) => {
            eprintln!("load generator configuration error: max_tasks must be greater than zero");
            return Ok(FunctionStatus::Failed);
        }
        None => {
            eprintln!("load generator configuration error: max_tasks is missing");
            return Ok(FunctionStatus::Failed);
        }
    };

    for i in 0..num_users {
        tasks.push(tokio::spawn(run_functions(
            param.functions_to_execute.clone(),
            kv_tx.clone(),
            param.timeout,
        )));

        let spawn_rate = eval_task_count(&eval_interpreter, &param.spawn_rate, tick)?.max(1) as u64;
        if (i + 1) % spawn_rate == 0 {
            sleep(Duration::from_secs(1)).await;
            tick += 1;
        }
    }

    let mut pass_count = 0;
    let mut fail_count = 0;
    let mut total_task_count = 0;
    let mut overall_status = FunctionStatus::Passed;

    for task_result in futures::future::join_all(tasks).await {
        total_task_count += 1;
        match task_result {
            Ok(Ok(FunctionStatus::Passed)) => pass_count += 1,
            Ok(Ok(FunctionStatus::Failed)) => {
                fail_count += 1;
                overall_status = FunctionStatus::Failed;
            }
            Ok(Err(err)) => {
                eprintln!("Task resolver returned error: {}", err);
                fail_count += 1;
                overall_status = FunctionStatus::Failed;
            }
            Err(join_err) => {
                eprintln!("Task join error: {}", join_err);
                fail_count += 1;
                overall_status = FunctionStatus::Failed;
            }
        };
    }
    println!("=== Load test complete ===");
    println!("TOTAL TASKS: {total_task_count}");
    println!("PASSED: {pass_count}");
    println!("FAILED: {fail_count}");

    let (resp_tx, resp_rx) = oneshot::channel();
    kv_tx
        .send(Command::Get {
            key: "load_gen_metrics".into(),
            resp: resp_tx,
        })
        .await?;
    let metrics = resp_rx.await??;

    if let Some(JsonValue::Array(metrics)) = metrics {
        println!("Collected metrics array size: {:?}", metrics.len());
        let metrics: Vec<HttpMetric> = metrics
            .into_iter()
            .map(serde_json::from_value)
            .collect::<std::result::Result<_, _>>()?;

        let json_str = serde_json::to_string(&metrics)?;

        let (resp_tx, resp_rx) = oneshot::channel();
        kv_tx
            .send(Command::Get {
                key: "metrics_output_path".into(),
                resp: resp_tx,
            })
            .await?;
        let metrics_output_value = resp_rx.await??;
        let metrics_output_path: PathBuf = metrics_output_value
            .and_then(|value| serde_json::from_value(value).ok())
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "metrics_output_path is missing or invalid",
                )
            })?;

        println!("Saving collected metrics to: {:?}", metrics_output_path);
        std::fs::write(metrics_output_path, json_str)?;
    } else {
        println!("It's a different value?!")
    }

    Ok(overall_status)
}
