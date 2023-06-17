mod flow;
mod functions;
mod kv_store;

use std::path::PathBuf;

use clap::Parser;

use crate::flow::Flow;
use crate::functions::run;
use kv_store::store::new as kv_store_new;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// A load generator written in Rust
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Flow config in json
    #[arg(long)]
    flow: Option<String>,

    /// Flow config file path
    #[arg(long)]
    flow_path: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    if args.flow.is_none() && args.flow_path.is_none() {
        println!("ERROR: one of 'flow' or 'flow_path' must be provided.");
        return Ok(());
    }

    let mut flow = args.flow.unwrap_or("".into());
    if let Some(path) = args.flow_path {
        flow = std::fs::read_to_string(path)?;
    }

    let flow: Flow = serde_json::from_str(&flow)?;
    let (kv_handle, kv_tx) = kv_store_new().await;

    run::run_flow(flow, kv_tx).await?;
    kv_handle.await?;

    Ok(())
}
