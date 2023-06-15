mod flow;
mod functions;
mod kv_store;

use kv_store::store::new as kv_store_new;

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
                    "spawn_rate": "1",
                    "timeout": 5,
                    "functions_to_execute": [
                        {
                            "HttpRequest": {
                                "url": "https://qa.zeuz.ai/Home/Dashboard",
                                "headers": [
                                    ["X-API-KEY", "d0808976-8be4-4d80-8d9d-5806f4ebb87c"]
                                ],
                                "timeout": 300
                            }
                        }
                    ]
                }
            }
        ]
    }
    "#;
    let flow: Flow = serde_json::from_str(param_str)?;
    let kv_tx = kv_store_new().await;

    run::run_flow(flow, kv_tx).await?;

    Ok(())
}
