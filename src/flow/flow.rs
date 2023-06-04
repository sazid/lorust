use serde::{Deserialize, Serialize};

use crate::functions::http_request::HttpRequestParam;

#[derive(Serialize, Deserialize, Debug)]
pub struct Flow {
    pub functions: Vec<Function>,
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
