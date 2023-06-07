use std::str::FromStr;
use std::time::Duration;

use isahc::http::Method;
use isahc::{config::RedirectPolicy, prelude::*, Request};
use isahc::{AsyncBody, HttpClient};

use form_data_builder::FormData;
use url_encoded_data::UrlEncodedData;

use serde::{Deserialize, Serialize};

use crate::kv_store::KvStore;

use super::result::*;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HttpMetric {
    /// URL of the request
    pub url: String,

    /// HTTP verb such as GET, POST, HEAD, PUT, etc.
    pub http_verb: String,

    /// HTTP status code of the response
    pub status_code: i64,

    /// Total size of the response body (in bytes)
    pub response_body_size: usize,

    /// When did the request start
    pub time_stamp: String,

    /// Whenever the status code is not within the range 200 <= 299,
    /// the response body is collected as a string.
    pub response_body: String,

    pub upload_total: f64,
    pub download_total: f64,
    pub upload_speed: f64,
    pub download_speed: f64,

    // An overview of the six time values (taken from the curl documentation):
    //
    // curl_easy_perform()                   struct member name
    //     |                                --------------------
    //     |--NAMELOOKUP                    - namelookup_time
    //     |--|--CONNECT                    - connect_time
    //     |--|--|--APPCONNECT              - tls_handshake_time
    //     |--|--|--|--PRETRANSFER
    //     |--|--|--|--|--STARTTRANSFER     - starttransfer_time
    //     |--|--|--|--|--|--TOTAL          - elapsed_time
    //     |--|--|--|--|--|--REDIRECT       - redirect_time
    //
    // The numbers we expose in the API are a little more "high-level" than the
    // ones written here.
    /// The total time from the start of the request until DNS name
    /// resolving was completed.
    ///
    /// When a redirect is followed, the time from each request is added
    /// together.
    pub namelookup_time: Duration,

    /// The amount of time taken to establish a connection to the server
    /// (not including TLS connection time).
    ///
    /// When a redirect is followed, the time from each request is added
    /// together.
    pub connect_time: Duration,

    /// Get the amount of time spent on TLS handshakes.
    ///
    /// When a redirect is followed, the time from each request is added
    /// together.
    pub tls_handshake_time: Duration,

    /// Get the time it took from the start of the request until the first
    /// byte is either sent or received.
    ///
    /// When a redirect is followed, the time from each request is added
    /// together.
    pub starttransfer_time: Duration,

    /// Get the total time for the entire request. This will continuously
    /// increase until the entire response body is consumed and completed.
    ///
    /// When a redirect is followed, the time from each request is added
    /// together.
    pub elapsed_time: Duration,

    /// If automatic redirect following is enabled, gets the total time taken
    /// for all redirection steps including name lookup, connect, pretransfer
    /// and transfer before final transaction was started.
    pub redirect_time: Duration,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum FormDataValue {
    Str(String),
    FilePath(String, String),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct KeyValue<T>(pub String, pub T);

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub enum HttpBody {
    #[default]
    Empty,
    Raw(String),
    FormData(Vec<KeyValue<FormDataValue>>),
    FormUrlEncoded(Vec<KeyValue<String>>),
    BinaryOctetFilePath(String),
}

fn default_http_method() -> String {
    "GET".into()
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HttpRequestParam {
    pub url: String,

    #[serde(default = "default_http_method")]
    pub method: String,

    #[serde(default)]
    pub headers: Vec<KeyValue<String>>,

    #[serde(default)]
    pub body: HttpBody,

    // #[serde(default)]
    // cookies: HashMap<String, String>,
    #[serde(default)]
    pub session: Option<String>,

    #[serde(default)]
    pub timeout: Option<u64>,
}

pub async fn make_request(param: HttpRequestParam, _kv_store: KvStore) -> Result {
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
    let response = client.send_async(request).await?;

    // println!("{}", response.text().await?);
    println!("{:#?}", response.metrics().unwrap());

    Ok(FunctionResult::Passed)
}
