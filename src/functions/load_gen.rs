use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::{
    flow::Function,
    functions::{http_request::HttpMetric, python_code, run::run_functions},
    kv_store::commands::{Command, Sender, Value},
};

use tokio::{
    sync::oneshot,
    time::{Duration, Instant, sleep_until},
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
    expression: &str,
    tick: i64,
) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
    python_code::eval_python_i64(
        expression,
        vec![("TICK".to_string(), JsonValue::from(tick))],
    )
}

fn percentile(sorted_values: &[u128], percentile: u128) -> u128 {
    if sorted_values.is_empty() {
        return 0;
    }

    let index = ((sorted_values.len() as u128 - 1) * percentile).div_ceil(100) as usize;
    sorted_values[index]
}

fn print_http_metric_summary(metrics: &[HttpMetric], elapsed: Duration) {
    if metrics.is_empty() {
        return;
    }

    let total_requests = metrics.len();
    let success_count = metrics
        .iter()
        .filter(|metric| (200..300).contains(&metric.status_code))
        .count();
    let failure_count = total_requests - success_count;

    let mut elapsed_times: Vec<u128> = metrics.iter().map(|metric| metric.elapsed_time).collect();
    elapsed_times.sort_unstable();

    let total_elapsed_time: u128 = elapsed_times.iter().sum();
    let avg_elapsed_time = total_elapsed_time as f64 / total_requests as f64;
    let rps = total_requests as f64 / elapsed.as_secs_f64().max(f64::EPSILON);
    let error_rate = failure_count as f64 * 100.0 / total_requests as f64;

    println!("=== HTTP metric summary ===");
    println!("REQUESTS: {total_requests}");
    println!("HTTP 2XX: {success_count}");
    println!("HTTP NON-2XX/ERROR: {failure_count}");
    println!("ERROR RATE: {error_rate:.2}%");
    println!("REQUESTS/SEC: {rps:.2}");
    println!("AVG LATENCY MS: {avg_elapsed_time:.2}");
    println!("P50 LATENCY MS: {}", percentile(&elapsed_times, 50));
    println!("P95 LATENCY MS: {}", percentile(&elapsed_times, 95));
    println!("P99 LATENCY MS: {}", percentile(&elapsed_times, 99));
}

pub async fn load_gen(param: LoadGenParam, kv_tx: Sender) -> FunctionResult {
    println!("Running load generator with the config:");
    let mut config_display = param.clone();
    config_display.functions_to_execute = Vec::new();
    println!("{:?}", config_display);

    let (resp_tx, resp_rx) = oneshot::channel();
    kv_tx
        .send(Command::SetArray {
            key: "load_gen_metrics".into(),
            value: Vec::new(),
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

    let schedule_started_at = Instant::now();
    let mut spawn_rate = eval_task_count(&param.spawn_rate, tick)?.max(1) as u64;
    let mut spawned_this_tick = 0;

    for i in 0..num_users {
        tasks.push(tokio::spawn(run_functions(
            param.functions_to_execute.clone(),
            kv_tx.clone(),
            param.timeout,
        )));

        if i + 1 == num_users {
            break;
        }

        spawned_this_tick += 1;
        let next_spawn_at = if spawned_this_tick >= spawn_rate {
            tick += 1;
            spawned_this_tick = 0;
            spawn_rate = eval_task_count(&param.spawn_rate, tick)?.max(1) as u64;
            schedule_started_at + Duration::from_secs(tick as u64)
        } else {
            let nanos_into_tick = spawned_this_tick * 1_000_000_000 / spawn_rate;
            schedule_started_at
                + Duration::from_secs(tick as u64)
                + Duration::from_nanos(nanos_into_tick)
        };

        sleep_until(next_spawn_at).await;
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

    if let Value::Json(JsonValue::Array(metrics)) = metrics {
        println!("Collected metrics array size: {:?}", metrics.len());
        let metrics: Vec<HttpMetric> = serde_json::from_value(JsonValue::Array(metrics))?;
        print_http_metric_summary(&metrics, schedule_started_at.elapsed());

        let json_str = serde_json::to_string(&metrics)?;

        let (resp_tx, resp_rx) = oneshot::channel();
        kv_tx
            .send(Command::Get {
                key: "metrics_output_path".into(),
                resp: resp_tx,
            })
            .await?;
        let Value::Json(metrics_output_path) = resp_rx.await??;
        let metrics_output_path = python_code::json_pathbuf(metrics_output_path)?;

        println!("Saving collected metrics to: {:?}", metrics_output_path);
        std::fs::write(metrics_output_path, json_str)?;
    } else {
        println!("It's a different value?!")
    }

    Ok(overall_status)
}
