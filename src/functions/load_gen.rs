use serde::{Deserialize, Serialize};

use crate::{flow::Function, functions::run::run_function};

use super::result::*;

#[derive(Serialize, Deserialize, Debug)]
pub struct LoadGenParam {
    spawn_rate: u32,

    max_users: u32,

    timeout: u32,

    max_workers: u32,

    functions_to_execute: Vec<Function>,
}

pub async fn load_gen(param: LoadGenParam) -> Result {
    println!("{:?}", param);
    run_function(param.functions_to_execute).await?;
    Ok(FunctionResult::Passed)
}
