mod flow;
mod functions;

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
                    "spawn_rate": "5 * TICK",
                    "timeout": 10,
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
    run::run_flow(flow).await?;

    Ok(())
}
