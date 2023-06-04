use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub enum FormDataValue {
    Str(String),
    FilePath(String, String),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct KeyValue<T>(pub String, pub T);

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
