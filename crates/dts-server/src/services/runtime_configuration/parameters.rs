use crate::{
    models::error::AppError,
    services::product_jobs::{
        parameters::{ParameterDefinition, resolve},
        validate_definition,
    },
};
use dts_backend::ProductResource;
use serde_json::{Value, json};

pub(super) fn validate_values(source: &ProductResource, values: &Value) -> Result<(), AppError> {
    validate_definition(&source.kind, &source.properties)?;
    let definitions = if source.kind == "development_workflow" {
        source.properties["definition"].get("parameters")
    } else {
        source.properties.get("parameters")
    };
    let definitions: Vec<ParameterDefinition> =
        serde_json::from_value(definitions.cloned().unwrap_or_else(|| json!([])))
            .map_err(|_| AppError::BadRequest("配置来源参数声明无效".into()))?;
    let map = values
        .as_object()
        .ok_or_else(|| AppError::BadRequest("变量必须是对象".into()))?;
    if map.len() > 32 {
        return Err(AppError::BadRequest("最多允许 32 个运行变量".into()));
    }
    for (name, value) in map {
        let definition = definitions
            .iter()
            .find(|definition| &definition.name == name)
            .ok_or_else(|| AppError::BadRequest("配置或覆盖变量没有对应的实际参数声明".into()))?;
        resolve(std::slice::from_ref(definition), &json!({name:value}))?;
    }
    Ok(())
}
