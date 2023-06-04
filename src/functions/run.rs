use crate::flow::{Flow, Function};

use super::http_request;
use super::result::*;
use super::sleep;

pub async fn run(flow: Flow) -> Result {
    for (index, function) in flow.functions.into_iter().enumerate() {
        println!("--- Running function #{} ---", index + 1);
        match function {
            Function::HttpRequest(param) => http_request::make_request(param).await?,
            Function::Sleep(param) => sleep::sleep(param).await?,
            Function::RunRhaiCode(_) => todo!(),
        };
    }

    let result = FunctionResult::Passed;
    Ok(result)
}
