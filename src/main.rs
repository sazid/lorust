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
                    "timeout": 1,
                    "functions_to_execute": [
                        {
                            "HttpRequest": {
                                "url": "https://reqres.in/api/users?page=1",
                                "timeout": 300
                            }
                        },
                        {
                            "RunRhaiCode": {
                                "code": "let user_id = http_response[\"data\"].sample().id;"
                            }
                        },
                        {
                            "RunRhaiCode": {
                                "code": "print(\"Picked user_id: \" + user_id);"
                            }
                        },
                        {
                            "HttpRequest": {
                                "url": "https://reqres.in/api/users/%|user_id|%",
                                "timeout": 300
                            }
                        },
                        {
                            "RunRhaiCode": {
                                "code": "let data = http_response.data; print(data.first_name + \" \" + data.last_name);"
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
