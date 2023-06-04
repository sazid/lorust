use serde::{Deserialize, Serialize};

use crate::functions::{http_request, load_gen, rhai_code, sleep};

#[derive(Serialize, Deserialize, Debug)]
pub struct Flow {
    pub functions: Vec<Function>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Function {
    HttpRequest(http_request::HttpRequestParam),
    Sleep(sleep::SleepParam),
    LoadGen(load_gen::LoadGenParam),
    RunRhaiCode(rhai_code::RhaiCodeParam),
    // Pick random item from li
    // Append item to ilst
}
