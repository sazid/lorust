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
    load_gen::LoadGenParam,
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
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run a JSON flow definition
    Run(FlowArgs),

    /// Run a simple HTTP load test without a flow file
    Http(HttpArgs),
}

#[derive(ClapArgs, Debug, Clone, Default)]
struct FlowArgs {
    /// Flow config in json
    #[arg(long)]
    flow: Option<String>,

    /// Flow config file path
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

    /// Total number of requests to execute
    #[arg(short = 'n', long, default_value_t = 1, value_parser = clap::value_parser!(u64).range(1..))]
    requests: u64,

    /// Requests to start per second
    #[arg(short = 'r', long, default_value_t = 1, value_parser = clap::value_parser!(u64).range(1..))]
    rate: u64,

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
    };

    let load_gen = LoadGenParam::new(
        args.rate.to_string(),
        args.timeout,
        Some(args.requests),
        vec![Function::HttpRequest(request)],
    );

    Ok(Flow {
        functions: vec![Function::LoadGen(load_gen)],
    })
}

fn flow_from_flow_args(args: FlowArgs) -> Result<Flow> {
    match (args.flow, args.flow_path) {
        (Some(_), Some(_)) => Err(boxed_error(
            "provide only one of --flow or --flow-path, not both",
        )),
        (Some(flow), None) => Ok(serde_json::from_str(&flow)?),
        (None, Some(path)) => Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?),
        (None, None) => Err(boxed_error(
            "provide --flow, --flow-path, or use `lorust http <url>`",
        )),
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

    run::run_flow(flow, kv_tx).await?;
    kv_handle.await?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let flow = match cli.command {
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

    execute_flow(flow, cli.output_path).await
}
