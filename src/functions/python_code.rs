use std::{io, path::PathBuf};

use rustpython_vm::{
    Interpreter, PyObjectRef, PyResult, VirtualMachine,
    builtins::PyBaseExceptionRef,
    eval,
    py_serde::{PyObjectDeserializer, PyObjectSerializer},
    scope::Scope,
};
use serde::{Deserialize, Serialize, de::DeserializeSeed};
use serde_json::Value as JsonValue;
use tokio::{sync::oneshot, task};

use crate::kv_store::commands::{Command, Sender, Value};

use super::result::*;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PythonCodeParam {
    code: String,
}

impl PythonCodeParam {
    pub fn new(code: String) -> Self {
        Self { code }
    }
}

fn boxed_error(message: impl Into<String>) -> Box<dyn std::error::Error + Send + Sync> {
    Box::new(io::Error::other(message.into()))
}

fn format_py_exception(vm: &VirtualMachine, err: PyBaseExceptionRef) -> String {
    let mut message = String::new();
    if vm.write_exception(&mut message, &err).is_err() {
        message = format!("{err:?}");
    }
    message.trim_end().to_string()
}

fn map_py_result<T>(vm: &VirtualMachine, result: PyResult<T>) -> Result<T> {
    result.map_err(|err| boxed_error(format_py_exception(vm, err)))
}

fn json_to_py(vm: &VirtualMachine, value: JsonValue) -> Result<PyObjectRef> {
    PyObjectDeserializer::new(vm)
        .deserialize(value)
        .map_err(|err| boxed_error(err.to_string()))
}

fn py_to_json(vm: &VirtualMachine, value: &PyObjectRef) -> Result<JsonValue> {
    serde_json::to_value(PyObjectSerializer::new(vm, value)).map_err(Into::into)
}

fn init_scope(vm: &VirtualMachine, values: Vec<(String, JsonValue)>) -> Result<Scope> {
    let scope = vm.new_scope_with_builtins();

    for (key, value) in values {
        let object = json_to_py(vm, value)?;
        map_py_result(vm, scope.globals.set_item(key.as_str(), object, vm))?;
    }

    Ok(scope)
}

fn should_store_global(key: &str) -> bool {
    key != "__builtins__" && !(key.starts_with("__") && key.ends_with("__"))
}

fn collect_scope_values(vm: &VirtualMachine, scope: &Scope) -> Result<Vec<(String, JsonValue)>> {
    let mut values = Vec::new();

    for (key, value) in &scope.globals {
        let Ok(key) = key.try_into_value::<String>(vm) else {
            continue;
        };

        if should_store_global(&key) {
            if let Ok(value) = py_to_json(vm, &value) {
                values.push((key, value));
            }
        }
    }

    Ok(values)
}

fn execute_python_code(
    code: &str,
    values: Vec<(String, JsonValue)>,
) -> Result<Vec<(String, JsonValue)>> {
    let interpreter = Interpreter::without_stdlib(Default::default());
    interpreter.enter(|vm| {
        let scope = init_scope(vm, values)?;
        map_py_result(
            vm,
            vm.run_string(scope.clone(), code, "<lorust>".to_string()),
        )?;
        collect_scope_values(vm, &scope)
    })
}

pub fn eval_python_expression_with_values(
    expression: &str,
    values: Vec<(String, JsonValue)>,
) -> Result<JsonValue> {
    let interpreter = Interpreter::without_stdlib(Default::default());
    interpreter.enter(|vm| {
        let scope = init_scope(vm, values)?;
        let value = map_py_result(vm, eval::eval(vm, expression, scope, "<lorust-eval>"))?;
        py_to_json(vm, &value)
    })
}

pub fn eval_python_i64(expression: &str, values: Vec<(String, JsonValue)>) -> Result<i64> {
    let value = eval_python_expression_with_values(expression, values)?;
    match value {
        JsonValue::Number(number) => {
            if let Some(value) = number.as_i64() {
                Ok(value)
            } else if let Some(value) = number.as_u64() {
                i64::try_from(value)
                    .map_err(|_| boxed_error("python expression result is too large"))
            } else if let Some(value) = number.as_f64() {
                Ok(value as i64)
            } else {
                Err(boxed_error(
                    "python expression did not return a usable number",
                ))
            }
        }
        JsonValue::String(value) => value.parse::<i64>().map_err(Into::into),
        _ => Err(boxed_error("python expression did not return a number")),
    }
}

async fn load_scope_values(local_kv_tx: &Sender) -> Result<Vec<(String, JsonValue)>> {
    let (resp_tx, resp_rx) = oneshot::channel();
    local_kv_tx
        .send(Command::ListKeys { resp: resp_tx })
        .await?;
    let keys = resp_rx.await??;

    let mut values = Vec::new();
    for key in keys {
        let (resp_tx, resp_rx) = oneshot::channel();
        local_kv_tx
            .send(Command::Get {
                key: key.clone(),
                resp: resp_tx,
            })
            .await?;

        let Value::Json(value) = resp_rx.await??;
        values.push((key, value));
    }

    Ok(values)
}

async fn store_scope_values(local_kv_tx: &Sender, values: Vec<(String, JsonValue)>) -> Result<()> {
    for (key, value) in values {
        let (resp_tx, resp_rx) = oneshot::channel();
        local_kv_tx
            .send(Command::Set {
                key,
                value,
                resp: resp_tx,
            })
            .await?;
        resp_rx.await??;
    }

    Ok(())
}

pub async fn run_python_code(
    param: PythonCodeParam,
    _global_kv_tx: Sender,
    local_kv_tx: Sender,
) -> FunctionResult {
    let values = load_scope_values(&local_kv_tx).await?;
    let code = param.code;
    let values = task::spawn_blocking(move || execute_python_code(&code, values)).await??;
    store_scope_values(&local_kv_tx, values).await?;

    Ok(FunctionStatus::Passed)
}

pub async fn eval_python_expression(code: &str, local_kv_tx: Sender) -> Result<JsonValue> {
    let values = load_scope_values(&local_kv_tx).await?;
    let code = code.to_string();
    task::spawn_blocking(move || eval_python_expression_with_values(&code, values)).await?
}

pub fn json_pathbuf(value: JsonValue) -> Result<PathBuf> {
    match value {
        JsonValue::String(path) => Ok(PathBuf::from(path)),
        value => Err(boxed_error(format!(
            "expected metrics_output_path to be a string, got {value}"
        ))),
    }
}
