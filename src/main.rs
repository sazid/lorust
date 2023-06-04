use std::str::FromStr;
use std::time::Duration;

use isahc::http::Method;
use isahc::{config::RedirectPolicy, prelude::*, Request};
use isahc::{AsyncBody, HttpClient};

use form_data_builder::FormData;
use url_encoded_data::UrlEncodedData;

mod flow;
mod functions;

use crate::flow::{Flow, Function};
use crate::functions::http_request::{FormDataValue, HttpBody, HttpRequestParam, KeyValue};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[tokio::main]
async fn main() -> Result<()> {
    let param_str = r#"
    {
        "functions": [
            {
                "HttpRequest": {
                    "url": "https://httpbin.org/ip"
                }
            }
        ]
    }
    "#;
    let flow: Flow = serde_json::from_str(param_str)?;
    for function in flow.functions {
        match function {
            Function::HttpRequest(param) => make_request(param).await?,
            Function::Sleep(_) => todo!(),
            Function::RunRhaiCode(_) => todo!(),
        }
    }

    Ok(())
}

async fn make_request(param: HttpRequestParam) -> Result<()> {
    let client = HttpClient::builder()
        .timeout(Duration::from_secs(60))
        .metrics(true)
        .redirect_policy(RedirectPolicy::Limit(5))
        .cookies()
        .build()
        .expect("failed to construct HttpClient");

    let mut request_builder = Request::builder()
        .uri(param.url)
        .method(Method::from_str(&param.method)?);

    for KeyValue(key, value) in param.headers {
        request_builder = request_builder.header(key, value);
    }

    if let Some(duration) = param.timeout {
        request_builder = request_builder.timeout(Duration::from_secs(duration));
    }

    let body = match param.body {
        HttpBody::Empty => AsyncBody::empty(),
        HttpBody::Raw(data) => AsyncBody::from(data),
        HttpBody::FormData(data) => {
            let mut form = FormData::new(Vec::new());

            for KeyValue(key, value) in data {
                match value {
                    FormDataValue::Str(value) => {
                        form.write_field(&key, &value)?;
                    }
                    FormDataValue::FilePath(path, content_type) => {
                        form.write_path(&key, &path, &content_type)?;
                    }
                }
            }

            request_builder = request_builder.header("Content-Type", form.content_type_header());

            AsyncBody::from(form.finish()?)
        }
        HttpBody::FormUrlEncoded(data) => {
            let mut encoded_data = UrlEncodedData::from("");

            for KeyValue(key, value) in &data {
                encoded_data.set_one(key, value);
            }

            AsyncBody::from(encoded_data.to_string())
        }
        HttpBody::BinaryOctetFilePath(_) => unimplemented!(),
    };

    let request = request_builder.body(body)?;
    let mut response = client.send_async(request).await?;

    println!("{}", response.text().await?);
    println!("{:#?}", response.metrics().unwrap());

    Ok(())
}
