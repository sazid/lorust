mod flow;
mod functions;
mod kv_store;

use std::io;
use std::path::PathBuf;

use clap::{Args as ClapArgs, Parser, Subcommand};
use kv_store::commands::Command;
use serde_json::Value as JsonValue;
use tokio::sync::oneshot;

use crate::flow::{Flow, Function};
use crate::functions::run;
use crate::functions::{
    http_request::{HttpBody, HttpRequestParam, KeyValue},
    load_gen::{HttpMetricThresholds, LoadGenParam},
};
use kv_store::store::new as kv_store_new;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn boxed_error(message: impl Into<String>) -> Box<dyn std::error::Error + Send + Sync> {
    Box::new(io::Error::other(message.into()))
}

/// A load generator written in Rust
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[command(flatten)]
    flow: FlowArgs,

    /// Write HTTP metrics JSON to this path
    #[arg(long, global = true, default_value_os_t = PathBuf::from("metrics_output"))]
    output_path: PathBuf,

    /// Logical run ID to attach to every emitted metric
    #[arg(long, global = true)]
    run_id: Option<String>,

    /// Worker ID to attach to every emitted metric
    #[arg(long, global = true)]
    worker_id: Option<String>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run a TOML or JSON flow definition
    Run(FlowArgs),

    /// Run a simple HTTP load test without a flow file
    Http(HttpArgs),
}

#[derive(ClapArgs, Debug, Clone, Default)]
struct FlowArgs {
    /// Flow config as inline JSON or TOML
    #[arg(long)]
    flow: Option<String>,

    /// Flow config file path. .toml and .json are supported.
    #[arg(long)]
    flow_path: Option<PathBuf>,
}

impl FlowArgs {
    fn has_flow_input(&self) -> bool {
        self.flow.is_some() || self.flow_path.is_some()
    }
}

#[derive(ClapArgs, Debug)]
struct HttpArgs {
    /// URL to request
    url: String,

    /// Total number of requests to execute. Defaults to 1 when --duration is absent.
    #[arg(short = 'n', long, conflicts_with = "duration", value_parser = clap::value_parser!(u64).range(1..))]
    requests: Option<u64>,

    /// Requests to start per second
    #[arg(short = 'r', long, default_value_t = 1, value_parser = clap::value_parser!(u64).range(1..))]
    rate: u64,

    /// How long to start requests for. Supports plain seconds, or suffixes s, m, h.
    #[arg(long, value_parser = parse_duration_secs)]
    duration: Option<u64>,

    /// HTTP method. Defaults to GET, or POST when --body is provided.
    #[arg(short = 'm', long)]
    method: Option<String>,

    /// Header in 'Name: value' format. Can be passed multiple times.
    #[arg(short = 'H', long = "header", value_name = "HEADER")]
    headers: Vec<String>,

    /// Raw request body
    #[arg(short = 'd', long)]
    body: Option<String>,

    /// Per-request timeout in seconds
    #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u64).range(1..))]
    timeout: u64,

    /// Maximum redirects to follow
    #[arg(long)]
    redirect_limit: Option<u32>,

    /// Maximum failed-response body bytes to store in metrics
    #[arg(long, default_value_t = 4096)]
    max_response_body_bytes: usize,

    /// Fail the run if error rate is above this percentage
    #[arg(long, value_parser = parse_percentage)]
    max_error_rate: Option<f64>,

    /// Fail the run if p95 latency is above this many milliseconds
    #[arg(long)]
    max_p95_ms: Option<u128>,

    /// Fail the run if p99 latency is above this many milliseconds
    #[arg(long)]
    max_p99_ms: Option<u128>,

    /// Fail the run if achieved requests/sec is below this value
    #[arg(long, value_parser = parse_non_negative_f64)]
    min_rps: Option<f64>,
}

fn parse_header(header: &str) -> Result<KeyValue<String>> {
    let (name, value) = header.split_once(':').ok_or_else(|| {
        boxed_error(format!(
            "invalid header '{header}', expected format 'Name: value'"
        ))
    })?;

    let name = name.trim();
    if name.is_empty() {
        return Err(boxed_error(format!(
            "invalid header '{header}', header name cannot be empty"
        )));
    }

    Ok(KeyValue(name.to_string(), value.trim_start().to_string()))
}

fn parse_duration_secs(value: &str) -> std::result::Result<u64, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("duration cannot be empty".into());
    }

    let (number, multiplier) = match value.as_bytes().last().copied() {
        Some(b's' | b'S') => (&value[..value.len() - 1], 1),
        Some(b'm' | b'M') => (&value[..value.len() - 1], 60),
        Some(b'h' | b'H') => (&value[..value.len() - 1], 60 * 60),
        Some(_) => (value, 1),
        None => return Err("duration cannot be empty".into()),
    };

    let number = number
        .trim()
        .parse::<u64>()
        .map_err(|_| format!("invalid duration '{value}'"))?;
    let duration = number
        .checked_mul(multiplier)
        .ok_or_else(|| format!("duration '{value}' is too large"))?;

    if duration == 0 {
        return Err("duration must be greater than zero".into());
    }

    Ok(duration)
}

fn parse_percentage(value: &str) -> std::result::Result<f64, String> {
    let parsed = value
        .parse::<f64>()
        .map_err(|_| format!("invalid percentage '{value}'"))?;
    if !(0.0..=100.0).contains(&parsed) {
        return Err("percentage must be between 0 and 100".into());
    }

    Ok(parsed)
}

fn parse_non_negative_f64(value: &str) -> std::result::Result<f64, String> {
    let parsed = value
        .parse::<f64>()
        .map_err(|_| format!("invalid number '{value}'"))?;
    if parsed < 0.0 {
        return Err("number must be greater than or equal to zero".into());
    }

    Ok(parsed)
}

fn flow_from_http_args(args: HttpArgs) -> Result<Flow> {
    let headers = args
        .headers
        .iter()
        .map(|header| parse_header(header))
        .collect::<Result<Vec<_>>>()?;
    let method = args.method.unwrap_or_else(|| {
        if args.body.is_some() {
            "POST".into()
        } else {
            "GET".into()
        }
    });
    let body = args.body.map(HttpBody::Raw).unwrap_or_default();

    let request = HttpRequestParam {
        url: args.url,
        method,
        headers,
        body,
        session: None,
        timeout: Some(args.timeout),
        redirect_limit: args.redirect_limit,
        max_response_body_bytes: Some(args.max_response_body_bytes),
    };

    let mut load_gen = LoadGenParam::new(
        args.rate.to_string(),
        args.timeout,
        args.requests
            .or_else(|| args.duration.is_none().then_some(1)),
        args.duration,
        vec![Function::HttpRequest(request)],
    );
    load_gen.set_thresholds(HttpMetricThresholds {
        max_error_rate: args.max_error_rate,
        max_p95_latency_ms: args.max_p95_ms,
        max_p99_latency_ms: args.max_p99_ms,
        min_requests_per_sec: args.min_rps,
    });

    Ok(Flow {
        functions: vec![Function::LoadGen(load_gen)],
    })
}

fn flow_from_flow_args(args: FlowArgs) -> Result<Flow> {
    match (args.flow, args.flow_path) {
        (Some(_), Some(_)) => Err(boxed_error(
            "provide only one of --flow or --flow-path, not both",
        )),
        (Some(flow), None) => parse_flow_source(&flow, None),
        (None, Some(path)) => {
            let source = std::fs::read_to_string(&path)?;
            let format = path
                .extension()
                .and_then(|extension| extension.to_str())
                .map(str::to_ascii_lowercase);
            parse_flow_source(&source, format.as_deref())
        }
        (None, None) => Err(boxed_error(
            "provide --flow, --flow-path, or use `lorust http <url>`",
        )),
    }
}

fn parse_flow_source(source: &str, format: Option<&str>) -> Result<Flow> {
    match format {
        Some("json") => Ok(serde_json::from_str(source)?),
        Some("toml") => crate::flow::from_toml_str(source),
        Some(format) => Err(boxed_error(format!(
            "unsupported flow file extension '.{format}', expected .toml or .json",
        ))),
        None => parse_json_or_toml(source),
    }
}

fn parse_json_or_toml(source: &str) -> Result<Flow> {
    match serde_json::from_str(source) {
        Ok(flow) => Ok(flow),
        Err(json_err) => match crate::flow::from_toml_str(source) {
            Ok(flow) => Ok(flow),
            Err(toml_err) => Err(boxed_error(format!(
                "flow was neither valid JSON nor TOML; JSON error: {json_err}; TOML error: {toml_err}",
            ))),
        },
    }
}

fn apply_run_metadata(flow: &mut Flow, run_id: Option<String>, worker_id: Option<String>) {
    if run_id.is_none() && worker_id.is_none() {
        return;
    }

    for function in &mut flow.functions {
        if let Function::LoadGen(param) = function {
            param.set_run_metadata(run_id.clone(), worker_id.clone());
        }
    }
}

async fn execute_flow(flow: Flow, output_path: PathBuf) -> Result<()> {
    let (kv_handle, kv_tx) = kv_store_new().await;

    let (resp_tx, resp_rx) = oneshot::channel();
    kv_tx
        .send(Command::Set {
            key: "metrics_output_path".into(),
            value: JsonValue::String(output_path.to_string_lossy().into_owned()),
            resp: resp_tx,
        })
        .await?;
    resp_rx.await??;

    let status = run::run_flow(flow, kv_tx).await?;
    kv_handle.await?;

    match status {
        functions::result::FunctionStatus::Passed => Ok(()),
        functions::result::FunctionStatus::Failed => Err(boxed_error("load test failed")),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut flow = match cli.command {
        Some(Commands::Run(args)) => flow_from_flow_args(args)?,
        Some(Commands::Http(args)) => {
            if cli.flow.has_flow_input() {
                return Err(boxed_error(
                    "--flow and --flow-path cannot be used with the http command",
                ));
            }
            flow_from_http_args(args)?
        }
        None => flow_from_flow_args(cli.flow)?,
    };
    apply_run_metadata(&mut flow, cli.run_id, cli.worker_id);

    execute_flow(flow, cli.output_path).await
}
