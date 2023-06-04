use crate::flow::{Flow, Function};

use super::http_request;
use super::load_gen;
use super::result::*;
use super::sleep;

pub async fn run_flow(flow: Flow) -> Result {
    run_loadgen(flow.functions).await?;

    Ok(FunctionResult::Passed)
}

pub async fn run_loadgen(functions: Vec<Function>) -> Result {
    for (index, function) in functions.into_iter().enumerate() {
        println!("--- Running function #{} ---", index + 1);
        match function {
            Function::LoadGen(param) => load_gen::load_gen(param.clone()).await?,
            _ => panic!("top level function must be loadgen"),
        };
    }

    Ok(FunctionResult::Passed)
}

pub async fn run_functions(functions: Vec<Function>) -> Result {
    for (_, function) in functions.into_iter().enumerate() {
        match function {
            Function::HttpRequest(param) => http_request::make_request(param.clone()).await?,
            Function::Sleep(param) => sleep::sleep(param.clone()).await?,
            Function::RunRhaiCode(_) => unimplemented!(),
            Function::LoadGen(_) => panic!("load gen function cannot be nested"),
        };
    }

    Ok(FunctionResult::Passed)
}
