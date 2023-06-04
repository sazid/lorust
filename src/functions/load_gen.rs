use serde::{Deserialize, Serialize};

use crate::{flow::Function, functions::run::run_functions};

use tokio::time::{interval, Duration, Instant};

use super::result::*;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LoadGenParam {
    spawn_rate: String,

    timeout: u64,

    functions_to_execute: Vec<Function>,
}

fn max(a: i64, b: i64) -> i64 {
    if a > b {
        a
    } else {
        b
    }
}

fn min(a: i64, b: i64) -> i64 {
    if a < b {
        a
    } else {
        b
    }
}

fn eval_task_count(
    expression: &str,
    tick: i64,
) -> std::result::Result<i64, Box<dyn std::error::Error + Send + Sync>> {
    let mut engine = rhai::Engine::new();
    engine.register_fn("max", max);
    engine.register_fn("min", min);

    let mut scope = rhai::Scope::new();
    scope.push_constant("TICK", tick);

    let result = engine
        .eval_expression_with_scope(&mut scope, expression)
        .unwrap();
    Ok(result)
}

pub async fn load_gen(param: LoadGenParam) -> Result {
    println!("Running load generator with the config:");
    println!("{:?}", param);

    let mut tasks = Vec::new();

    let timeout_time = Instant::now() + Duration::from_secs(param.timeout);
    let mut interval = interval(Duration::from_secs(1));
    let mut tick = 0_i64;
    while Instant::now() <= timeout_time {
        interval.tick().await;
        tick += 1;
        println!("=== TICK #{tick} ===");

        let task_count: i64 = eval_task_count(&param.spawn_rate, tick)?;

        for _ in 0..task_count {
            tasks.push(tokio::spawn(run_functions(
                param.functions_to_execute.clone(),
            )));
        }

        println!("TASK COUNT: {task_count}");
    }

    let mut pass_count = 0;
    let mut fail_count = 0;
    let mut total_task_count = 0;
    for task_result in futures::future::join_all(tasks).await {
        match task_result?? {
            FunctionResult::Failed => fail_count += 1,
            FunctionResult::Passed => pass_count += 1,
        };
        total_task_count += 1;
    }
    println!("Load test complete.");
    println!("TOTAL TASKS: {total_task_count}");
    println!("PASSED: {pass_count}");
    println!("FAILED: {fail_count}");

    Ok(FunctionResult::Passed)
}
