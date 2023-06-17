use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub enum FunctionStatus {
    Passed,
    Failed,
}

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub type FunctionResult = Result<FunctionStatus>;
