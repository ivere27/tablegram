use serde_json::json;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn parse_recordset_json(bytes: &[u8]) -> String {
    match tablegram::parse_recordset_bytes(bytes) {
        Ok(recordset) => serde_json::to_string(&recordset).unwrap_or_else(|err| {
            json!({
                "error": format!("failed to encode JSON: {}", err)
            })
            .to_string()
        }),
        Err(err) => json!({ "error": err.to_string() }).to_string(),
    }
}
