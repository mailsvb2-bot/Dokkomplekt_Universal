use crate::core_error::{CoreError, CoreResult};
use serde::Serialize;
use serde_json::Value;

pub fn canonical_json<T: Serialize>(value: &T) -> CoreResult<Vec<u8>> {
    let json_value =
        serde_json::to_value(value).map_err(|exc| CoreError::BadCanonicalJson(exc.to_string()))?;
    let normalized = normalize_value(json_value);
    serde_json::to_vec(&normalized).map_err(|exc| CoreError::BadCanonicalJson(exc.to_string()))
}

fn normalize_value(value: Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.into_iter().map(normalize_value).collect()),
        Value::Object(map) => {
            let mut sorted = serde_json::Map::new();
            let mut pairs: Vec<_> = map.into_iter().collect();
            pairs.sort_by(|left, right| left.0.cmp(&right.0));
            for (key, item) in pairs {
                sorted.insert(key, normalize_value(item));
            }
            Value::Object(sorted)
        }
        item => item,
    }
}
