use crate::models::error::AppError;
use serde_json::Value;
use std::collections::BTreeSet;

pub fn validate_definition(properties: &Value) -> Result<(), AppError> {
    if properties["object_id"]
        .as_str()
        .is_none_or(|id| dts_backend::repo::quality_alerts::validation::identifier(id).is_err())
    {
        return Err(AppError::BadRequest("监控对象须为真实质量对象标识".into()));
    }
    if !properties["failure_threshold"]
        .as_f64()
        .is_some_and(|value| value.is_finite() && (0.0..=1.0).contains(&value))
    {
        return Err(AppError::BadRequest("监控阈值须为0至1有限数".into()));
    }
    if !properties["window_hours"]
        .as_i64()
        .is_some_and(|hours| (1..=8760).contains(&hours))
    {
        return Err(AppError::BadRequest("监控窗口须为1至8760小时整数".into()));
    }
    if let Some(value) = properties
        .get("recipient_ids")
        .filter(|value| !value.is_null())
    {
        let recipients = value
            .as_array()
            .filter(|values| values.len() <= 100)
            .ok_or_else(|| AppError::BadRequest("收件人须为最多100个真实用户标识".into()))?;
        let mut seen = BTreeSet::new();
        for recipient in recipients {
            let id = recipient
                .as_str()
                .filter(|id| dts_backend::repo::quality_alerts::validation::identifier(id).is_ok())
                .ok_or_else(|| AppError::BadRequest("收件人标识格式无效".into()))?;
            if !seen.insert(id) {
                return Err(AppError::BadRequest("告警收件人不能重复".into()));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn invalid_monitor_cannot_be_saved_before_evaluation() {
        let mut definition = json!({"object_id":"010fb4e0-7a38-44d1-bbc4-e0938e43d053","failure_threshold":0.2,"window_hours":24});
        assert!(validate_definition(&definition).is_ok());
        for (field, value) in [
            ("failure_threshold", json!(1.1)),
            ("window_hours", json!(1.5)),
            ("window_hours", json!(0)),
            ("object_id", json!("")),
            ("recipient_ids", json!([""])),
        ] {
            let original = definition[field].clone();
            definition[field] = value;
            assert!(validate_definition(&definition).is_err());
            definition[field] = original;
        }
        definition["recipient_ids"] = json!([
            "010fb4e0-7a38-44d1-bbc4-e0938e43d053",
            "010fb4e0-7a38-44d1-bbc4-e0938e43d053"
        ]);
        assert!(validate_definition(&definition).is_err());
    }
}
