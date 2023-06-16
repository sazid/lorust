use crate::flow::{Flow, Function};
use crate::kv_store::{commands::Sender, store::new as kv_store_new};

use super::http_request;
use super::load_gen;
use super::result::*;
use super::rhai_code;
use super::sleep;

pub async fn run_flow(flow: Flow, kv_tx: Sender) -> Result {
    run_loadgen(flow.functions, kv_tx.clone()).await?;

    Ok(FunctionResult::Passed)
}

pub async fn run_loadgen(functions: Vec<Function>, kv_tx: Sender) -> Result {
    for (index, function) in functions.into_iter().enumerate() {
        println!("--- Running function #{} ---", index + 1);
        match function {
            Function::LoadGen(param) => load_gen::load_gen(param.clone(), kv_tx.clone()).await?,
            _ => panic!("top level function must be loadgen"),
        };
    }

    Ok(FunctionResult::Passed)
}

pub async fn run_functions(functions: Vec<Function>, global_kv_tx: Sender) -> Result {
    // TODO: Instead of defining something like this, there should be proper
    // scoping mechanisms with scope names that can be referred from inside
    // functions. Maybe a graph of scopes that child scopes can refer back to?
    let local_kv_tx = kv_store_new().await;

    for (_, function) in functions.into_iter().enumerate() {
        match function {
            Function::HttpRequest(param) => {
                http_request::make_request(param.clone(), global_kv_tx.clone(), local_kv_tx.clone())
                    .await?
            }
            Function::Sleep(param) => sleep::sleep(param.clone(), global_kv_tx.clone()).await?,
            Function::RunRhaiCode(param) => {
                rhai_code::run_rhai_code(param, global_kv_tx.clone(), local_kv_tx.clone()).await?
            }
            Function::LoadGen(_) => panic!("load gen function cannot be nested"),
        };
    }

    Ok(FunctionResult::Passed)
}
