use serde::Serialize;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub struct JsonRpcRequest {
  pub id: u64,
  pub method: String,
  pub params: Option<Value>
}

impl JsonRpcRequest {
  pub fn new(method: impl Into<String>, params: Option<Value>) -> Self {
    Self { id: NEXT_ID.fetch_add(1, Ordering::Relaxed), method: method.into(), params }
  }

  pub fn to_line(&self) -> Result<String, serde_json::Error> {
    let payload = json!({
      "jsonrpc": "2.0",
      "id": self.id,
      "method": self.method,
      "params": self.params.clone().unwrap_or_else(|| json!({}))
    });
    Ok(format!("{}\n", serde_json::to_string(&payload)?))
  }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeEventKind { JsonRpc, Stderr, Terminated, Error }

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeEvent { pub kind: RuntimeEventKind, pub payload: Value }

pub fn parse_json_line(line: &str) -> Option<Value> { serde_json::from_str(line.trim()).ok() }
