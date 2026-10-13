use super::{AppError, FrozenResource, ProductResource, Value};

pub(super) fn frozen(resource: &ProductResource) -> FrozenResource {
    FrozenResource {
        id: resource.id.clone(),
        version: resource.version,
        name: resource.name.clone(),
        properties: resource.properties.clone(),
    }
}
pub(super) fn bad(message: impl Into<String>) -> AppError {
    AppError::BadRequest(message.into())
}
pub(super) fn text<'a>(properties: &'a Value, key: &str) -> Result<&'a str, AppError> {
    properties
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| {
            !value.trim().is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
        })
        .ok_or_else(|| bad(format!("缺少有效{key}")))
}
pub(super) fn ids(properties: &Value, key: &str, required: bool) -> Result<Vec<String>, AppError> {
    let Some(values) = properties.get(key) else {
        return if required {
            Err(bad(format!("缺少{key}")))
        } else {
            Ok(vec![])
        };
    };
    let values = values
        .as_array()
        .filter(|values| values.len() <= 64 && (!required || !values.is_empty()))
        .ok_or_else(|| bad(format!("{key}必须是最多64项的不重复ID数组")))?;
    let mut seen = std::collections::HashSet::new();
    values
        .iter()
        .map(|value| {
            let id = value
                .as_str()
                .filter(|id| !id.is_empty() && id.len() <= 128 && *id == id.trim())
                .ok_or_else(|| bad(format!("{key}含无效ID")))?;
            if !seen.insert(id) {
                return Err(bad(format!("{key}含重复ID")));
            }
            Ok(id.into())
        })
        .collect()
}
