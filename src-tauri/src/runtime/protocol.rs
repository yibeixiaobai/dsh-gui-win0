use serde::Serialize;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub struct JsonRpcRequest {
  pub id: u64,
  pub method: String,
  pub params: Option<Value>,
}

impl JsonRpcRequest {
  pub fn new(method: impl Into<String>, params: Option<Value>) -> Self {
    Self {
      id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
      method: method.into(),
      params,
    }
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
pub struct RuntimeEvent {
  pub kind: RuntimeEventKind,
  pub payload: Value,
}

pub fn parse_json_line(line: &str) -> Option<Value> {
  serde_json::from_str(line.trim()).ok()
}

pub fn response_id(value: &Value) -> Option<u64> {
  value.get("id").and_then(Value::as_u64)
}

pub fn is_response(value: &Value) -> bool {
  value.get("id").is_some()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn request_is_one_json_line() {
    let request = JsonRpcRequest::new("initialize", Some(json!({"model":"x"})));
    let line = request.to_line().expect("serialize");
    assert!(line.ends_with('\n'));
    let parsed = parse_json_line(&line).expect("parse");
    assert_eq!(parsed["jsonrpc"], "2.0");
    assert_eq!(parsed["method"], "initialize");
    assert_eq!(parsed["id"].as_u64(), Some(request.id));
  }

  #[test]
  fn response_helpers_detect_id() {
    let response = json!({"jsonrpc":"2.0","id":7,"result":{}});
    assert!(is_response(&response));
    assert_eq!(response_id(&response), Some(7));
  }
}
