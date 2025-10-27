use serde::{Deserialize, Serialize};

use crate::functions::{http_request, load_gen, python_code, sleep};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Flow {
    pub functions: Vec<Function>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Function {
    HttpRequest(http_request::HttpRequestParam),
    Sleep(sleep::SleepParam),
    LoadGen(load_gen::LoadGenParam),
    RunPythonCode(python_code::PythonCodeParam),
    // Pick random item from li
    // Append item to ilst
}
