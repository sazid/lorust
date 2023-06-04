use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub enum FunctionResult {
    Passed,
    Failed,
}

pub type Result = std::result::Result<FunctionResult, Box<dyn std::error::Error>>;
