use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct RpcRequest {
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct RpcResponse {
    pub jsonrpc: &'static str,
    pub id: Value,
    pub result: Value,
}

impl RpcResponse {
    pub fn new(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result,
        }
    }
}
