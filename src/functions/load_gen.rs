use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::{
    flow::Function,
    functions::{
        http_request::HttpMetric,
        python_code,
        run::{TaskContext, run_functions},
    },
    kv_store::commands::{Command, Sender, Value},
};

use tokio::{
    sync::oneshot,
    time::{Duration, Instant, sleep_until},
};

use super::result::*;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LoadGenParam {
    #[serde(default)]
    run_id: Option<String>,

    #[serde(default)]
    worker_id: Option<String>,

    spawn_rate: String,

    timeout: u64,

    #[serde(default)]
    max_tasks: Option<u64>,

    #[serde(default)]
    duration: Option<u64>,

    #[serde(default)]
    thresholds: HttpMetricThresholds,

    functions_to_execute: Vec<Function>,
}

impl LoadGenParam {
    pub fn new(
        spawn_rate: String,
        timeout: u64,
        max_tasks: Option<u64>,
        duration: Option<u64>,
        functions_to_execute: Vec<Function>,
    ) -> Self {
        Self {
            run_id: None,
            worker_id: None,
            spawn_rate,
            timeout,
            max_tasks,
            duration,
            thresholds: HttpMetricThresholds::default(),
            functions_to_execute,
        }
    }

    pub fn set_run_metadata(&mut self, run_id: Option<String>, worker_id: Option<String>) {
        if let Some(run_id) = run_id {
            self.run_id = Some(run_id);
        }

        if let Some(worker_id) = worker_id {
            self.worker_id = Some(worker_id);
        }
    }

    pub fn set_thresholds(&mut self, thresholds: HttpMetricThresholds) {
        self.thresholds = thresholds;
    }
}

fn default_run_id() -> String {
    chrono::Utc::now()
        .format("run-%Y%m%dT%H%M%S%.fZ")
        .to_string()
}

fn default_worker_id() -> String {
    "local".into()
}

async fn eval_task_count(
    expression: &str,
    tick: i64,
) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
    let expression = expression.to_string();
    tokio::task::spawn_blocking(move || {
        python_code::eval_python_i64(
            &expression,
            vec![("TICK".to_string(), JsonValue::from(tick))],
        )
    })
    .await?
}

fn percentile(sorted_values: &[u128], percentile: u128) -> u128 {
    if sorted_values.is_empty() {
        return 0;
    }

    let index = ((sorted_values.len() as u128 - 1) * percentile).div_ceil(100) as usize;
    sorted_values[index]
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HttpMetricSummary {
    pub total_requests: usize,
    pub success_count: usize,
    pub failure_count: usize,
    pub error_rate: f64,
    pub requests_per_sec: f64,
    pub avg_latency_ms: f64,
    pub p50_latency_ms: u128,
    pub p95_latency_ms: u128,
    pub p99_latency_ms: u128,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct HttpMetricThresholds {
    #[serde(default)]
    pub max_error_rate: Option<f64>,

    #[serde(default)]
    pub max_p95_latency_ms: Option<u128>,

    #[serde(default)]
    pub max_p99_latency_ms: Option<u128>,

    #[serde(default)]
    pub min_requests_per_sec: Option<f64>,
}

impl HttpMetricSummary {
    pub fn from_metrics(metrics: &[HttpMetric], elapsed: Duration) -> Option<Self> {
        if metrics.is_empty() {
            return None;
        }

        let total_requests = metrics.len();
        let success_count = metrics
            .iter()
            .filter(|metric| (200..300).contains(&metric.status_code))
            .count();
        let failure_count = total_requests - success_count;

        let mut elapsed_times: Vec<u128> =
            metrics.iter().map(|metric| metric.elapsed_time).collect();
        elapsed_times.sort_unstable();

        let total_elapsed_time: u128 = elapsed_times.iter().sum();
        let avg_latency_ms = total_elapsed_time as f64 / total_requests as f64;
        let requests_per_sec = total_requests as f64 / elapsed.as_secs_f64().max(f64::EPSILON);
        let error_rate = failure_count as f64 * 100.0 / total_requests as f64;

        Some(Self {
            total_requests,
            success_count,
            failure_count,
            error_rate,
            requests_per_sec,
            avg_latency_ms,
            p50_latency_ms: percentile(&elapsed_times, 50),
            p95_latency_ms: percentile(&elapsed_times, 95),
            p99_latency_ms: percentile(&elapsed_times, 99),
        })
    }

    pub fn print(&self) {
        println!("=== HTTP metric summary ===");
        println!("REQUESTS: {}", self.total_requests);
        println!("HTTP 2XX: {}", self.success_count);
        println!("HTTP NON-2XX/ERROR: {}", self.failure_count);
        println!("ERROR RATE: {:.2}%", self.error_rate);
        println!("REQUESTS/SEC: {:.2}", self.requests_per_sec);
        println!("AVG LATENCY MS: {:.2}", self.avg_latency_ms);
        println!("P50 LATENCY MS: {}", self.p50_latency_ms);
        println!("P95 LATENCY MS: {}", self.p95_latency_ms);
        println!("P99 LATENCY MS: {}", self.p99_latency_ms);
    }
}

impl HttpMetricThresholds {
    fn has_thresholds(&self) -> bool {
        self.max_error_rate.is_some()
            || self.max_p95_latency_ms.is_some()
            || self.max_p99_latency_ms.is_some()
            || self.min_requests_per_sec.is_some()
    }

    fn evaluate(&self, summary: &HttpMetricSummary) -> Vec<String> {
        let mut failures = Vec::new();

        if let Some(max_error_rate) = self.max_error_rate {
            if summary.error_rate > max_error_rate {
                failures.push(format!(
                    "error rate {:.2}% exceeded max {:.2}%",
                    summary.error_rate, max_error_rate
                ));
            }
        }

        if let Some(max_p95_latency_ms) = self.max_p95_latency_ms {
            if summary.p95_latency_ms > max_p95_latency_ms {
                failures.push(format!(
                    "p95 latency {}ms exceeded max {}ms",
                    summary.p95_latency_ms, max_p95_latency_ms
                ));
            }
        }

        if let Some(max_p99_latency_ms) = self.max_p99_latency_ms {
            if summary.p99_latency_ms > max_p99_latency_ms {
                failures.push(format!(
                    "p99 latency {}ms exceeded max {}ms",
                    summary.p99_latency_ms, max_p99_latency_ms
                ));
            }
        }

        if let Some(min_requests_per_sec) = self.min_requests_per_sec {
            if summary.requests_per_sec < min_requests_per_sec {
                failures.push(format!(
                    "requests/sec {:.2} was below min {:.2}",
                    summary.requests_per_sec, min_requests_per_sec
                ));
            }
        }

        failures
    }
}

fn sort_http_metrics(metrics: &mut [HttpMetric]) {
    metrics.sort_by(|left, right| {
        left.started_at_nanos
            .cmp(&right.started_at_nanos)
            .then_with(|| left.worker_id.cmp(&right.worker_id))
            .then_with(|| left.task_id.cmp(&right.task_id))
            .then_with(|| left.sequence.cmp(&right.sequence))
    });
}

pub async fn load_gen(param: LoadGenParam, kv_tx: Sender) -> FunctionResult {
    println!("Running load generator with the config:");
    let mut config_display = param.clone();
    config_display.functions_to_execute = Vec::new();
    println!("{:?}", config_display);

    let run_id = param.run_id.clone().unwrap_or_else(default_run_id);
    let worker_id = param.worker_id.clone().unwrap_or_else(default_worker_id);
    println!("RUN ID: {run_id}");
    println!("WORKER ID: {worker_id}");

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

    let max_tasks = match param.max_tasks {
        Some(value) if value > 0 => Some(value),
        Some(_) => {
            eprintln!("load generator configuration error: max_tasks must be greater than zero");
            return Ok(FunctionStatus::Failed);
        }
        None => None,
    };
    let duration = match param.duration {
        Some(value) if value > 0 => Some(Duration::from_secs(value)),
        Some(_) => {
            eprintln!("load generator configuration error: duration must be greater than zero");
            return Ok(FunctionStatus::Failed);
        }
        None => None,
    };

    if max_tasks.is_none() && duration.is_none() {
        eprintln!("load generator configuration error: max_tasks or duration must be provided");
        return Ok(FunctionStatus::Failed);
    }

    let schedule_started_at = Instant::now();
    let schedule_ends_at = duration.map(|duration| schedule_started_at + duration);
    let mut tick = 0;
    let mut spawn_rate = eval_task_count(&param.spawn_rate, tick).await?.max(1) as u64;
    let mut spawned_this_tick = 0;
    let mut next_task_id = 1;

    loop {
        if max_tasks.is_some_and(|max_tasks| next_task_id > max_tasks) {
            break;
        }

        if schedule_ends_at.is_some_and(|schedule_ends_at| Instant::now() >= schedule_ends_at) {
            break;
        }

        let task_context = TaskContext {
            run_id: run_id.clone(),
            worker_id: worker_id.clone(),
            task_id: next_task_id,
        };
        tasks.push(tokio::spawn(run_functions(
            param.functions_to_execute.clone(),
            kv_tx.clone(),
            param.timeout,
            task_context,
        )));
        next_task_id += 1;

        if max_tasks.is_some_and(|max_tasks| next_task_id > max_tasks) {
            break;
        }

        spawned_this_tick += 1;
        let next_spawn_at = if spawned_this_tick >= spawn_rate {
            tick += 1;
            spawned_this_tick = 0;
            spawn_rate = eval_task_count(&param.spawn_rate, tick).await?.max(1) as u64;
            schedule_started_at + Duration::from_secs(tick as u64)
        } else {
            let nanos_into_tick = spawned_this_tick * 1_000_000_000 / spawn_rate;
            schedule_started_at
                + Duration::from_secs(tick as u64)
                + Duration::from_nanos(nanos_into_tick)
        };

        if let Some(schedule_ends_at) = schedule_ends_at {
            if next_spawn_at >= schedule_ends_at {
                sleep_until(schedule_ends_at).await;
                break;
            }
        }

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
        let mut metrics: Vec<HttpMetric> = serde_json::from_value(JsonValue::Array(metrics))?;
        sort_http_metrics(&mut metrics);
        if let Some(summary) =
            HttpMetricSummary::from_metrics(&metrics, schedule_started_at.elapsed())
        {
            summary.print();
            let threshold_failures = param.thresholds.evaluate(&summary);
            if threshold_failures.is_empty() {
                if param.thresholds.has_thresholds() {
                    println!("=== Thresholds passed ===");
                }
            } else {
                println!("=== Threshold failures ===");
                for failure in threshold_failures {
                    println!("{failure}");
                }
                overall_status = FunctionStatus::Failed;
            }
        }

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
