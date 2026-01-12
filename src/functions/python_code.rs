use std::collections::BTreeMap;
use std::fmt;

use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict};
use pyo3::Bound;
use pyo3::PyErr;
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

fn map_py_err(err: PyErr) -> Box<dyn std::error::Error + Send + Sync> {
    python_error(err.to_string())
}

#[derive(Clone)]
pub struct PythonInterpreter {
    globals: Py<PyDict>,
}

impl PythonInterpreter {
    pub fn new() -> Result<Self> {
        Python::with_gil(|py| -> PyResult<Self> {
            let globals = PyDict::new_bound(py);
            let builtins = py.import_bound("builtins")?;
            globals.set_item("__builtins__", builtins)?;

            Ok(Self {
                globals: globals.unbind(),
            })
        })
        .map_err(map_py_err)
    }

    fn json_to_python(&self, py: Python<'_>, value: &JsonValue) -> Result<Py<PyAny>> {
        let json_str = serde_json::to_string(value)
            .map_err(|err| python_error(format!("failed to serialize JSON for Python: {err}")))?;
        let json_module = py.import_bound("json").map_err(map_py_err)?;
        let loads = json_module.getattr("loads").map_err(map_py_err)?;
        let loaded = loads.call1((json_str,)).map_err(map_py_err)?;
        Ok(loaded.unbind())
    }

    fn python_to_json(&self, py: Python<'_>, value: &Bound<'_, PyAny>) -> Result<JsonValue> {
        let json_module = py.import_bound("json").map_err(map_py_err)?;
        let dumps = json_module.getattr("dumps").map_err(map_py_err)?;
        let json_str: String = dumps
            .call1((value,))
            .map_err(map_py_err)?
            .extract()
            .map_err(map_py_err)?;
        serde_json::from_str(&json_str).map_err(|err| {
            python_error(format!("failed to deserialize Python value to JSON: {err}"))
        })
    }

    fn sync_context(
        &self,
        py: Python<'_>,
        globals: &Bound<'_, PyDict>,
        context: &BTreeMap<String, JsonValue>,
    ) -> Result<()> {
        for (key, value) in context {
            let py_value = self.json_to_python(py, value)?;
            globals.set_item(key, py_value).map_err(map_py_err)?;
        }
        Ok(())
    }

    fn collect_globals(
        &self,
        py: Python<'_>,
        globals: &Bound<'_, PyDict>,
    ) -> Result<BTreeMap<String, JsonValue>> {
        let mut values = BTreeMap::new();

        for (key, value) in globals.iter() {
            let key_str = match key.extract::<String>() {
                Ok(name) => name,
                Err(err) => {
                    eprintln!("Skipping Python scope entry with non-string key: {}", err);
                    continue;
                }
            };
            if key_str.starts_with("__") {
                continue;
            }

            match self.python_to_json(py, &value) {
                Ok(json_value) => {
                    values.insert(key_str, json_value);
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

    pub fn run_script(
        &self,
        code: &str,
        context: &BTreeMap<String, JsonValue>,
    ) -> Result<BTreeMap<String, JsonValue>> {
        Python::with_gil(|py| -> Result<BTreeMap<String, JsonValue>> {
            let globals = self.globals.bind(py);
            self.sync_context(py, &globals, context)?;
            py.run_bound(code, Some(&globals), Some(&globals))
                .map_err(map_py_err)?;
            self.collect_globals(py, &globals)
        })
    }

    pub fn eval_expression(
        &self,
        code: &str,
        context: &BTreeMap<String, JsonValue>,
    ) -> Result<JsonValue> {
        Python::with_gil(|py| -> Result<JsonValue> {
            let globals = self.globals.bind(py);
            self.sync_context(py, &globals, context)?;
            let result = py
                .eval_bound(code, Some(&globals), Some(&globals))
                .map_err(map_py_err)?;
            self.python_to_json(py, &result)
        })
    }
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
            variables.insert(key, value);
        }
    }

    Ok(variables)
}

async fn delete_variable(local_kv_tx: &Sender, key: &str) -> Result<()> {
    let (resp_tx, resp_rx) = oneshot::channel();
    local_kv_tx
        .send(Command::Delete {
            key: key.to_owned(),
            resp: resp_tx,
        })
        .await?;
    resp_rx.await??;
    Ok(())
}

pub async fn run_python_code(
    param: PythonCodeParam,
    python: PythonInterpreter,
    local_kv_tx: Sender,
) -> FunctionResult {
    let variables = collect_variables(&local_kv_tx).await?;
    let existing_keys: Vec<String> = variables.keys().cloned().collect();
    let updated_values = python.run_script(&param.code, &variables)?;

    for key in existing_keys {
        if !updated_values.contains_key(&key) {
            delete_variable(&local_kv_tx, &key).await?;
        }
    }

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

pub async fn eval_python_code(
    code: &str,
    python: PythonInterpreter,
    local_kv_tx: Sender,
) -> Result<JsonValue> {
    let variables = collect_variables(&local_kv_tx).await?;
    python.eval_expression(code, &variables)
}

pub fn eval_expression_with_context(
    python: &PythonInterpreter,
    code: &str,
    context: &BTreeMap<String, JsonValue>,
) -> Result<JsonValue> {
    python.eval_expression(code, context)
}
