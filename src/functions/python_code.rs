use std::collections::BTreeMap;
use std::fmt;

use rustpython_stdlib::get_module_inits;
use rustpython_vm::builtins::PyBaseExceptionRef;
use rustpython_vm::py_serde::{self, PyObjectSerializer};
use rustpython_vm::scope::Scope;
use rustpython_vm::{AsObject, Interpreter, PyObjectRef, PyResult, VirtualMachine};
use serde::de::IntoDeserializer;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tokio::sync::oneshot;

use crate::kv_store::commands::{Command, Sender};

use super::result::*;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PythonCodeParam {
    pub code: String,
}

#[derive(Debug)]
struct PythonError(String);

impl fmt::Display for PythonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for PythonError {}

fn python_error(message: String) -> Box<dyn std::error::Error + Send + Sync> {
    Box::new(PythonError(message))
}

fn format_py_err(vm: &VirtualMachine, err: PyBaseExceptionRef) -> String {
    err.as_object()
        .repr(vm)
        .map(|value| value.as_str().to_owned())
        .unwrap_or_else(|_| err.class().name().to_string())
}

fn map_py_err<T>(vm: &VirtualMachine, result: PyResult<T>) -> Result<T> {
    result.map_err(|err| python_error(format_py_err(vm, err)))
}

fn json_to_py(vm: &VirtualMachine, value: &JsonValue) -> Result<PyObjectRef> {
    let deserializer = value.clone().into_deserializer();
    py_serde::deserialize(vm, deserializer)
        .map_err(|err| python_error(format!("failed to deserialize json to python value: {err}")))
}

fn py_to_json(vm: &VirtualMachine, value: &PyObjectRef) -> Result<JsonValue> {
    let serializer = PyObjectSerializer::new(vm, value);
    serde_json::to_value(serializer)
        .map_err(|err| python_error(format!("failed to serialize python value to json: {err}")))
}

fn build_scope(vm: &VirtualMachine, variables: &BTreeMap<String, JsonValue>) -> Result<Scope> {
    let scope = vm.new_scope_with_builtins();

    for (key, value) in variables {
        let py_value = json_to_py(vm, value)?;
        map_py_err(vm, scope.globals.set_item(key.as_str(), py_value, vm))?;
    }

    Ok(scope)
}

fn extract_scope_variables(
    vm: &VirtualMachine,
    scope: &Scope,
) -> Result<BTreeMap<String, JsonValue>> {
    let mut values = BTreeMap::new();

    for (key_obj, value_obj) in scope.globals.clone().into_iter() {
        let key = match map_py_err(vm, key_obj.str(vm)) {
            Ok(name) => name,
            Err(err) => {
                eprintln!("Skipping Python scope entry with non-string key: {err}");
                continue;
            }
        };
        let key_str = key.as_str();
        if key_str.starts_with("__") {
            continue;
        }

        match py_to_json(vm, &value_obj) {
            Ok(json_value) => {
                values.insert(key_str.to_owned(), json_value);
            }
            Err(err) => {
                eprintln!(
                    "Skipping non-serializable Python variable '{}': {}",
                    key_str, err
                );
            }
        }
    }

    Ok(values)
}

async fn collect_variables(local_kv_tx: &Sender) -> Result<BTreeMap<String, JsonValue>> {
    let (resp_tx, resp_rx) = oneshot::channel();
    local_kv_tx
        .send(Command::ListKeys { resp: resp_tx })
        .await?;
    let keys = resp_rx.await??;

    let mut variables = BTreeMap::new();
    for key in keys {
        let (resp_tx, resp_rx) = oneshot::channel();
        local_kv_tx
            .send(Command::Get {
                key: key.clone(),
                resp: resp_tx,
            })
            .await?;

        if let Some(value) = resp_rx.await?? {
            let value = if let JsonValue::String(raw) = value {
                serde_json::from_str(&raw).unwrap_or(JsonValue::String(raw))
            } else {
                value
            };
            variables.insert(key, value);
        }
    }

    Ok(variables)
}

fn with_interpreter<F, R>(variables: &BTreeMap<String, JsonValue>, action: F) -> Result<R>
where
    F: FnOnce(&VirtualMachine, Scope) -> Result<R>,
{
    let interpreter = Interpreter::with_init(Default::default(), |vm| {
        vm.add_native_modules(get_module_inits());
    });

    interpreter.enter(|vm| {
        let scope = build_scope(vm, variables)?;
        action(vm, scope)
    })
}

pub async fn run_python_code(
    param: PythonCodeParam,
    _global_kv_tx: Sender,
    local_kv_tx: Sender,
) -> FunctionResult {
    let variables = collect_variables(&local_kv_tx).await?;

    let updated_values = with_interpreter(&variables, |vm, scope| {
        map_py_err(
            vm,
            vm.run_code_string(scope.clone(), &param.code, "<embedded>".to_owned()),
        )?;
        extract_scope_variables(vm, &scope)
    })?;

    for (key, value) in updated_values {
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

    Ok(FunctionStatus::Passed)
}

pub async fn eval_python_code(code: &str, local_kv_tx: Sender) -> Result<JsonValue> {
    let variables = collect_variables(&local_kv_tx).await?;
    eval_expression_with_context(code, &variables)
}

pub fn eval_expression_with_context(
    code: &str,
    context: &BTreeMap<String, JsonValue>,
) -> Result<JsonValue> {
    with_interpreter(context, |vm, scope| {
        let result = map_py_err(vm, vm.run_block_expr(scope, code))?;
        py_to_json(vm, &result)
    })
}
