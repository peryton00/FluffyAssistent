use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct IpcMessage {
    pub schema_version: String,
    pub payload: serde_json::Value,
}
