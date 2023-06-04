use serde::{Deserialize, Serialize};

use crate::functions::{http_request, rhai_code, sleep};

#[derive(Serialize, Deserialize, Debug)]
pub struct Flow {
    pub functions: Vec<Function>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Function {
    HttpRequest(http_request::HttpRequestParam),
    Sleep(sleep::SleepParam),
    RunRhaiCode(rhai_code::RhaiCodeParam),
    // Pick random item from list
    // Append item to ilst
}
