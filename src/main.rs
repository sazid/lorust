use std::str::FromStr;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use isahc::http::Method;
use isahc::{config::RedirectPolicy, prelude::*, Request};
use isahc::{AsyncBody, HttpClient};

use form_data_builder::FormData;
use url_encoded_data::UrlEncodedData;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[tokio::main]
async fn main() -> Result<()> {
    // let client = HttpClient::builder()
    //     .timeout(Duration::from_secs(60))
    //     .metrics(true)
    //     .redirect_policy(RedirectPolicy::Limit(5))
    //     .cookies()
    //     .build()
    //     .expect("failed to construct HttpClient");

    // let request = Request::builder()
    //     .uri("https://qa.zeuz.ai/zsvc/tc/v1/TEST-0152/json")
    //     .method("GET")
    //     .header("X-API-KEY", "d0808976-8be4-4d80-8d9d-5806f4ebb87c")
    //     .redirect_policy(RedirectPolicy::Limit(5))
    //     .body(())?;

    // let mut response = client.send_async(request).await?;
    // println!("{}", response.text().await?);
    // println!("{:#?}", response.metrics().unwrap());

    // let flow = Flow {
    //     functions: vec![Function::HttpRequest(HttpRequestParam {
    //         url: "https://qa.zeuz.ai/zsvc/tc/v1/TEST-0152/json".into(),
    //         method: "GET".into(),
    //         headers: Vec::new(),
    //         body: HttpBody::Empty,
    //         session: None,
    //         timeout: None,
    //     })],
    // };
    // println!("{:#?}", serde_json::to_string(&flow)?);
    // let param = HttpRequestParam {
    //     url: "https://qa.zeuz.ai/zsvc/tc/v1/TEST-0152/json".into(),
    //     method: "GET".into(),
    //     headers: Vec::new(),
    //     body: HttpBody::Empty,
    //     session: None,
    //     timeout: None,
    // };
    // println!("{:#?}", serde_json::to_string(&param)?);

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

#[derive(Serialize, Deserialize, Debug)]
pub struct Flow {
    pub functions: Vec<Function>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum FormDataValue {
    Str(String),
    FilePath(String, String),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct KeyValue<T>(String, T);

#[derive(Serialize, Deserialize, Debug)]
pub enum HttpBody {
    Empty,
    Raw(String),
    FormData(Vec<KeyValue<FormDataValue>>),
    FormUrlEncoded(Vec<KeyValue<String>>),
    BinaryOctetFilePath(String),
}

impl std::default::Default for HttpBody {
    fn default() -> Self {
        HttpBody::Empty
    }
}

fn default_http_method() -> String {
    "GET".into()
}

#[derive(Serialize, Deserialize, Debug)]
pub struct HttpRequestParam {
    url: String,

    #[serde(default = "default_http_method")]
    method: String,

    #[serde(default)]
    headers: Vec<KeyValue<String>>,

    #[serde(default)]
    body: HttpBody,

    // #[serde(default)]
    // cookies: HashMap<String, String>,
    #[serde(default)]
    session: Option<String>,

    #[serde(default)]
    timeout: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SleepParam {
    duration_in_secs: i32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RhaiCodeParam {
    code: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Function {
    HttpRequest(HttpRequestParam),
    Sleep(SleepParam),
    RunRhaiCode(RhaiCodeParam),
    // Pick random item from list
    // Append item to ilst
}
