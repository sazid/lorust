mod flow;
mod functions;

use crate::flow::Flow;
use crate::functions::run;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[tokio::main]
async fn main() -> Result<()> {
    let param_str = r#"
    {
        "functions": [
            {
                "Sleep": {
                    "duration": 5
                }
            },
            {
                "HttpRequest": {
                    "url": "https://httpbin.org/ip"
                }
            }
        ]
    }
    "#;
    let flow: Flow = serde_json::from_str(param_str)?;
    run::run(flow).await?;

    Ok(())
}
