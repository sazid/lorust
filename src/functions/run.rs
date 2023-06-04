use crate::flow::{Flow, Function};
use async_recursion::async_recursion;

use super::http_request;
use super::load_gen;
use super::result::*;
use super::sleep;

pub async fn run_flow(flow: Flow) -> Result {
    run_function(flow.functions).await?;

    Ok(FunctionResult::Passed)
}

#[async_recursion]
pub async fn run_function(functions: Vec<Function>) -> Result {
    for (index, function) in functions.into_iter().enumerate() {
        println!("--- Running function #{} ---", index + 1);
        match function {
            Function::HttpRequest(param) => http_request::make_request(param).await?,
            Function::Sleep(param) => sleep::sleep(param).await?,
            Function::RunRhaiCode(_) => unimplemented!(),
            Function::LoadGen(param) => load_gen::load_gen(param).await?,
        };
    }

    Ok(FunctionResult::Passed)
}
