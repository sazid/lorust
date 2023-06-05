mod flow;
mod functions;
mod kv_store;

use kv_store::KvStore;

use crate::flow::Flow;
use crate::functions::run;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[tokio::main]
async fn main() -> Result<()> {
    let param_str = r#"
    {
        "functions": [
            {
                "LoadGen": {
                    "spawn_rate": "1 * TICK",
                    "max_tasks": 3,
                    "timeout": 5,
                    "functions_to_execute": [
                        {
                            "HttpRequest": {
                                "url": "https://qa.zeuz.ai/Home/Dashboard",
                                "headers": [
                                    ["X-API-KEY", "d0808976-8be4-4d80-8d9d-5806f4ebb87c"]
                                ]
                            }
                        }
                    ]
                }
            }
        ]
    }
    "#;
    let flow: Flow = serde_json::from_str(param_str)?;
    let kv_store = KvStore::new();

    run::run_flow(flow, kv_store.clone()).await?;

    Ok(())
}
