//! Pure JSON-RPC 2.0 envelope helpers (verbatim port of `core_jsonrpc.rs`).

use anyhow::{Result, bail};
use serde_json::Value;

use super::McpSessionState;

pub(crate) fn ensure_initialized(state: &McpSessionState) -> Result<()> {
    if state.initialized {
        Ok(())
    } else {
        bail!("server not initialized")
    }
}

pub(crate) fn jsonrpc_result(id: Value, result: Value) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    })
}

pub(crate) fn jsonrpc_error(id: Option<Value>, code: i64, message: &str) -> Value {
    let mut obj = serde_json::json!({
        "jsonrpc": "2.0",
        "error": {
            "code": code,
            "message": message,
        }
    });
    if let Some(id) = id {
        obj.as_object_mut()
            .expect("json object")
            .insert("id".to_string(), id);
    }
    obj
}
