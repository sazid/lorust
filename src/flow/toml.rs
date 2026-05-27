use std::{collections::BTreeMap, io};

use serde::Deserialize;

use crate::{
    flow::{Flow, Function},
    functions::{
        http_request::{FormDataValue, HttpBody, HttpRequestParam, KeyValue},
        load_gen::{HttpMetricThresholds, LoadGenParam},
        python_code::PythonCodeParam,
        sleep::SleepParam,
    },
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Deserialize)]
struct TomlFlow {
    #[serde(default)]
    loadgen: Vec<TomlLoadGen>,
}

#[derive(Deserialize)]
struct TomlLoadGen {
    #[serde(default)]
    run_id: Option<String>,

    #[serde(default)]
    worker_id: Option<String>,

    spawn_rate: String,
    timeout: u64,

    #[serde(default)]
    max_tasks: Option<u64>,

    #[serde(default)]
    duration: Option<u64>,

    #[serde(default)]
    thresholds: HttpMetricThresholds,

    #[serde(default)]
    step: Vec<TomlStep>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum TomlStep {
    Http(TomlHttpStep),
    Python(TomlPythonStep),
    Sleep(TomlSleepStep),
}

#[derive(Deserialize)]
struct TomlHttpStep {
    url: String,

    #[serde(default = "default_http_method")]
    method: String,

    #[serde(default)]
    headers: BTreeMap<String, String>,

    #[serde(default)]
    body: Option<String>,

    #[serde(default)]
    form_data: Option<BTreeMap<String, TomlFormDataValue>>,

    #[serde(default)]
    form_urlencoded: Option<BTreeMap<String, String>>,

    #[serde(default)]
    session: Option<String>,

    #[serde(default)]
    timeout: Option<u64>,

    #[serde(default)]
    redirect_limit: Option<u32>,

    #[serde(default)]
    max_response_body_bytes: Option<usize>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum TomlFormDataValue {
    Str(String),
    FilePath {
        file_path: String,
        content_type: String,
    },
}

#[derive(Deserialize)]
struct TomlPythonStep {
    code: String,
}

#[derive(Deserialize)]
struct TomlSleepStep {
    duration: String,
}

fn default_http_method() -> String {
    "GET".into()
}

fn boxed_error(message: impl Into<String>) -> Box<dyn std::error::Error + Send + Sync> {
    Box::new(io::Error::other(message.into()))
}

pub fn from_toml_str(input: &str) -> Result<Flow> {
    let flow: TomlFlow = ::toml::from_str(input)?;
    flow.try_into()
}

impl TryFrom<TomlFlow> for Flow {
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn try_from(flow: TomlFlow) -> Result<Self> {
        if flow.loadgen.is_empty() {
            return Err(boxed_error(
                "TOML flow must contain at least one [[loadgen]]",
            ));
        }

        let functions = flow
            .loadgen
            .into_iter()
            .map(TomlLoadGen::try_into)
            .collect::<Result<Vec<_>>>()?;

        Ok(Self { functions })
    }
}

impl TryFrom<TomlLoadGen> for Function {
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn try_from(loadgen: TomlLoadGen) -> Result<Self> {
        if loadgen.step.is_empty() {
            return Err(boxed_error(
                "TOML load generator must contain at least one [[loadgen.step]]",
            ));
        }

        let steps = loadgen
            .step
            .into_iter()
            .map(TomlStep::try_into)
            .collect::<Result<Vec<_>>>()?;

        let mut param = LoadGenParam::new(
            loadgen.spawn_rate,
            loadgen.timeout,
            loadgen.max_tasks,
            loadgen.duration,
            steps,
        );
        param.set_run_metadata(loadgen.run_id, loadgen.worker_id);
        param.set_thresholds(loadgen.thresholds);

        Ok(Function::LoadGen(param))
    }
}

impl TryFrom<TomlStep> for Function {
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn try_from(step: TomlStep) -> Result<Self> {
        match step {
            TomlStep::Http(step) => step.try_into(),
            TomlStep::Python(step) => Ok(Function::RunPythonCode(PythonCodeParam::new(step.code))),
            TomlStep::Sleep(step) => Ok(Function::Sleep(SleepParam::new(step.duration))),
        }
    }
}

impl TryFrom<TomlHttpStep> for Function {
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn try_from(step: TomlHttpStep) -> Result<Self> {
        let body_count = step.body.is_some() as u8
            + step.form_data.is_some() as u8
            + step.form_urlencoded.is_some() as u8;
        if body_count > 1 {
            return Err(boxed_error(
                "HTTP TOML step can contain only one of body, form_data, or form_urlencoded",
            ));
        }

        let headers = step
            .headers
            .into_iter()
            .map(|(key, value)| KeyValue(key, value))
            .collect();
        let body = match (step.body, step.form_data, step.form_urlencoded) {
            (Some(body), None, None) => HttpBody::Raw(body),
            (None, Some(form_data), None) => {
                let form_data = form_data
                    .into_iter()
                    .map(|(key, value)| {
                        let value = match value {
                            TomlFormDataValue::Str(value) => FormDataValue::Str(value),
                            TomlFormDataValue::FilePath {
                                file_path,
                                content_type,
                            } => FormDataValue::FilePath(file_path, content_type),
                        };
                        KeyValue(key, value)
                    })
                    .collect();
                HttpBody::FormData(form_data)
            }
            (None, None, Some(form_urlencoded)) => {
                let form_urlencoded = form_urlencoded
                    .into_iter()
                    .map(|(key, value)| KeyValue(key, value))
                    .collect();
                HttpBody::FormUrlEncoded(form_urlencoded)
            }
            (None, None, None) => HttpBody::Empty,
            _ => unreachable!("body_count validation rejects multiple bodies"),
        };

        Ok(Function::HttpRequest(HttpRequestParam {
            url: step.url,
            method: step.method,
            headers,
            body,
            session: step.session,
            timeout: step.timeout,
            redirect_limit: step.redirect_limit,
            max_response_body_bytes: step.max_response_body_bytes,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_checked_in_flow_toml() {
        from_toml_str(include_str!("../../flow.toml")).unwrap();
    }

    #[test]
    fn converts_toml_flow_to_runtime_flow() {
        let flow = from_toml_str(
            r#"
[[loadgen]]
max_tasks = 1
spawn_rate = "1"
timeout = 5

[loadgen.thresholds]
max_error_rate = 1.0

[[loadgen.step]]
type = "http"
method = "POST"
url = "https://example.com"
headers = { "Content-Type" = "application/json" }
body = '''
{"name":"Ada"}
'''
timeout = 10

[[loadgen.step]]
type = "python"
code = '''
user_id = http_response["data"][0]["id"]
'''

[[loadgen.step]]
type = "sleep"
duration = "1"
"#,
        )
        .unwrap();

        let value = serde_json::to_value(flow).unwrap();
        let loadgen = &value["functions"][0]["LoadGen"];
        assert_eq!(loadgen["max_tasks"], 1);
        assert_eq!(loadgen["thresholds"]["max_error_rate"], 1.0);

        let steps = &loadgen["functions_to_execute"];
        assert_eq!(
            steps[0]["HttpRequest"]["body"]["Raw"],
            "{\"name\":\"Ada\"}\n"
        );
        assert_eq!(
            steps[1]["RunPythonCode"]["code"],
            "user_id = http_response[\"data\"][0][\"id\"]\n"
        );
        assert_eq!(steps[2]["Sleep"]["duration"], "1");
    }
}
