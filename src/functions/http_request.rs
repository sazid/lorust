use std::collections::BTreeMap;
use std::str::FromStr;
use std::time::Duration;
use std::time::Instant as StdInstant;

use isahc::http::Method;
use isahc::{AsyncBody, AsyncReadResponseExt, HttpClient};
use isahc::{Request, config::RedirectPolicy, prelude::*, tls::TlsConfig};

use form_data_builder::FormData;
use tokio::sync::oneshot;
use url_encoded_data::UrlEncodedData;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::kv_store::commands::{Command, Sender};

use super::result::*;

async fn set_local_value(local_kv_tx: &Sender, key: &str, value: JsonValue) -> Result<()> {
    let (resp_tx, resp_rx) = oneshot::channel();
    local_kv_tx
        .send(Command::Set {
            key: key.to_string(),
            value,
            resp: resp_tx,
        })
        .await?;
    resp_rx.await??;
    Ok(())
}

async fn append_metric(global_kv_tx: &Sender, metric: HttpMetric) -> Result<()> {
    let (resp_tx, resp_rx) = oneshot::channel();
    global_kv_tx
        .send(Command::Append {
            key: "load_gen_metrics".into(),
            value: serde_json::to_value(metric)?,
            resp: resp_tx,
        })
        .await?;
    resp_rx.await??;
    Ok(())
}

pub async fn should_collect_metrics(global_kv_tx: &Sender) -> Result<bool> {
    let (resp_tx, resp_rx) = oneshot::channel();
    global_kv_tx
        .send(Command::Exists {
            key: "load_gen_metrics".into(),
            resp: resp_tx,
        })
        .await?;
    Ok(resp_rx.await??)
}

pub fn new_client(should_collect_metrics: bool) -> Result<HttpClient> {
    Ok(HttpClient::builder()
        .metrics(should_collect_metrics)
        .cookies()
        .tls_config(
            TlsConfig::builder()
                .danger_accept_invalid_certs(true)
                .danger_accept_invalid_hosts(true)
                .build(),
        )
        .build()?)
}

fn headers_to_json(headers: &isahc::http::HeaderMap) -> Result<JsonValue> {
    let headers: BTreeMap<String, String> = headers
        .iter()
        .filter_map(|(k, v)| {
            v.to_str()
                .ok()
                .map(|value| (k.to_string(), value.to_string()))
        })
        .collect();

    Ok(serde_json::to_value(headers)?)
}

fn body_to_json(body: &[u8]) -> JsonValue {
    serde_json::from_slice(body)
        .unwrap_or_else(|_| JsonValue::String(String::from_utf8_lossy(body).into_owned()))
}

async fn record_http_error(
    url: &str,
    method: &str,
    time_stamp: String,
    error_message: String,
    status_code: Option<i64>,
    headers_json: Option<JsonValue>,
    elapsed_time: u128,
    should_collect_metrics: bool,
    global_kv_tx: &Sender,
    local_kv_tx: &Sender,
) -> Result<()> {
    set_local_value(
        local_kv_tx,
        "http_response",
        JsonValue::String(error_message.clone()),
    )
    .await?;
    set_local_value(
        local_kv_tx,
        "http_status_code",
        JsonValue::from(status_code.unwrap_or(0)),
    )
    .await?;
    let headers_json = headers_json.unwrap_or_else(|| JsonValue::Object(Default::default()));
    set_local_value(local_kv_tx, "http_response_headers", headers_json).await?;

    if should_collect_metrics {
        let metric = HttpMetric {
            url: url.to_string(),
            http_verb: method.to_string(),
            status_code: status_code.unwrap_or(0),
            response_body_size: 0,
            time_stamp,
            response_body: error_message,
            upload_total: 0,
            download_total: 0,
            upload_speed: 0.0,
            download_speed: 0.0,
            namelookup_time: 0,
            connect_time: 0,
            tls_handshake_time: 0,
            starttransfer_time: 0,
            elapsed_time,
            redirect_time: 0,
        };

        append_metric(global_kv_tx, metric).await?;
    }

    Ok(())
}

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

    pub upload_total: u64,
    pub download_total: u64,
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
    pub namelookup_time: u128,

    /// The amount of time taken to establish a connection to the server
    /// (not including TLS connection time).
    ///
    /// When a redirect is followed, the time from each request is added
    /// together.
    pub connect_time: u128,

    /// Get the amount of time spent on TLS handshakes.
    ///
    /// When a redirect is followed, the time from each request is added
    /// together.
    pub tls_handshake_time: u128,

    /// Get the time it took from the start of the request until the first
    /// byte is either sent or received.
    ///
    /// When a redirect is followed, the time from each request is added
    /// together.
    pub starttransfer_time: u128,

    /// Get the total time for the entire request. This will continuously
    /// increase until the entire response body is consumed and completed.
    ///
    /// When a redirect is followed, the time from each request is added
    /// together.
    pub elapsed_time: u128,

    /// If automatic redirect following is enabled, gets the total time taken
    /// for all redirection steps including name lookup, connect, pretransfer
    /// and transfer before final transaction was started.
    pub redirect_time: u128,
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

    #[serde(default)]
    pub session: Option<String>,

    #[serde(default)]
    pub timeout: Option<u64>,

    #[serde(default)]
    pub redirect_limit: Option<u32>,
}

pub async fn make_request(
    param: HttpRequestParam,
    timeout: Option<Duration>,
    client: HttpClient,
    should_collect_metrics: bool,
    global_kv_tx: Sender,
    local_kv_tx: Sender,
) -> FunctionResult {
    // timeout from the parameters of this request
    let param_timeout = Duration::from_secs(param.timeout.unwrap_or(60));
    let timeout = match timeout {
        Some(t) => std::cmp::min(param_timeout, t),
        None => param_timeout,
    };

    let metrics_url = param.url.clone();
    let metrics_method = param.method.clone();

    let mut request_builder = Request::builder()
        .uri(metrics_url.clone())
        .method(Method::from_str(&metrics_method)?)
        .timeout(timeout)
        .redirect_policy(RedirectPolicy::Limit(param.redirect_limit.unwrap_or(5)));

    for KeyValue(key, value) in param.headers {
        request_builder = request_builder.header(key, value);
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

    let time_stamp = chrono::Local::now()
        .format("%Y-%m-%d %H:%M:%S.%f")
        .to_string();
    let started_at = StdInstant::now();
    let mut response = match client.send_async(request).await {
        Ok(response) => response,
        Err(err) => {
            let error_message = format!("Request failed: {}", err);
            record_http_error(
                &metrics_url,
                &metrics_method,
                time_stamp.clone(),
                error_message,
                None,
                None,
                started_at.elapsed().as_millis(),
                should_collect_metrics,
                &global_kv_tx,
                &local_kv_tx,
            )
            .await?;

            return Ok(FunctionStatus::Failed);
        }
    };

    // WARNING: The response body can be read only once.
    let body = match response.bytes().await {
        Ok(body) => body,
        Err(err) => {
            let status_code = response.status().as_u16() as i64;
            let headers_json = headers_to_json(response.headers())?;
            let error_message = format!("Failed to read response body: {}", err);
            record_http_error(
                &metrics_url,
                &metrics_method,
                time_stamp.clone(),
                error_message,
                Some(status_code),
                Some(headers_json),
                started_at.elapsed().as_millis(),
                should_collect_metrics,
                &global_kv_tx,
                &local_kv_tx,
            )
            .await?;

            return Ok(FunctionStatus::Failed);
        }
    };

    let status = response.status();
    let status_is_success = status.is_success();
    let status_code = status.as_u16() as i64;
    let response_body = String::from_utf8_lossy(&body);

    set_local_value(&local_kv_tx, "http_response", body_to_json(&body)).await?;
    set_local_value(
        &local_kv_tx,
        "http_status_code",
        JsonValue::from(status_code),
    )
    .await?;

    let headers_json = headers_to_json(response.headers())?;
    set_local_value(&local_kv_tx, "http_response_headers", headers_json).await?;

    // Collect metrics if the key is set.
    if should_collect_metrics {
        let response_body: String = if status_is_success {
            String::new()
        } else {
            response_body.into_owned()
        };

        let http_metrics = response
            .metrics()
            .expect("metrics must be set to true in the builder");

        let metric = HttpMetric {
            url: metrics_url.clone(),
            http_verb: metrics_method.clone(),
            status_code,
            response_body_size: body.len(),
            time_stamp,
            response_body,

            upload_total: http_metrics.upload_progress().0,
            download_total: http_metrics.download_progress().0,
            upload_speed: http_metrics.upload_speed(),
            download_speed: http_metrics.download_speed(),

            namelookup_time: http_metrics.name_lookup_time().as_millis(),
            connect_time: http_metrics.connect_time().as_millis(),
            tls_handshake_time: http_metrics.secure_connect_time().as_millis(),
            starttransfer_time: http_metrics.transfer_start_time().as_millis(),
            elapsed_time: http_metrics.total_time().as_millis(),
            redirect_time: http_metrics.redirect_time().as_millis(),
        };

        append_metric(&global_kv_tx, metric).await?;
    }

    // println!("{}", response.text().await?);
    // println!("{:#?}", response.metrics());
    // println!("{:#?}", param.url);

    if status_is_success {
        Ok(FunctionStatus::Passed)
    } else {
        Ok(FunctionStatus::Failed)
    }
}
