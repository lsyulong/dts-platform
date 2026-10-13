// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use serde::Serialize;
use serde_json::Value;

use super::parameters::bad;
use crate::models::error::AppError;

#[derive(Serialize)]
pub struct WorkflowScheduleDefinition {
    pub cron_expression: String,
    pub timezone: String,
    pub retry_limit: u64,
    pub automatic_retry_enabled: bool,
    pub scheduler_timezone_supported: bool,
}

/// Definition validation is independent of remote registration or dispatch.
/// Legacy manual DAGs may omit the entire schedule, but not half of it.
pub(crate) fn validate(properties: &Value) -> Result<Option<WorkflowScheduleDefinition>, AppError> {
    let cron = properties.get("cron_expression").filter(|v| !v.is_null());
    let timezone = properties.get("timezone").filter(|v| !v.is_null());
    let retry = properties.get("retry_limit").filter(|v| !v.is_null());
    if cron.is_none() && timezone.is_none() && retry.is_none() {
        return Ok(None);
    }
    let cron = cron
        .and_then(Value::as_str)
        .ok_or_else(|| bad("周期定义缺少有效 Cron 表达式"))?;
    let timer = dts_scheduler_client::models::TimerExpr::from_cron(cron)
        .map_err(|error| bad(&error.to_string()))?;
    let timezone = timezone
        .and_then(Value::as_str)
        .filter(|v| v.len() <= 128 && !v.is_empty())
        .ok_or_else(|| bad("周期定义缺少有效 IANA 时区"))?;
    let parsed = timezone
        .parse::<chrono_tz::Tz>()
        .map_err(|_| bad("周期时区必须为有效 IANA 名称，例如 UTC 或 Asia/Shanghai"))?;
    let retry_limit = match retry {
        Some(value) => value
            .as_u64()
            .filter(|v| *v <= 10)
            .ok_or_else(|| bad("周期重试次数必须为 0 至 10 的整数"))?,
        None => 0,
    };
    Ok(Some(WorkflowScheduleDefinition {
        cron_expression: timer.as_custom().expr,
        timezone: parsed.to_string(),
        retry_limit,
        automatic_retry_enabled: false,
        // Community Jia currently maps every non-utc value to worker Local.
        // A valid IANA definition must never be advertised as registered there.
        scheduler_timezone_supported: parsed == chrono_tz::UTC,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validates_whole_schedule_without_claiming_registration_or_retry() {
        let result = validate(&json!({"cron_expression":"0 8 * JAN MON-FRI","timezone":"Asia/Shanghai","retry_limit":3})).unwrap().unwrap();
        assert_eq!(result.cron_expression, "0 0 8 * 1 1-5");
        assert_eq!(result.timezone, "Asia/Shanghai");
        assert_eq!(result.retry_limit, 3);
        assert!(!result.automatic_retry_enabled);
        assert!(!result.scheduler_timezone_supported);
        assert!(
            validate(&json!({"cron_expression":"0 8 * * *","timezone":"UTC"}))
                .unwrap()
                .unwrap()
                .scheduler_timezone_supported
        );
        assert!(validate(&json!({})).unwrap().is_none());
    }

    #[test]
    fn rejects_partial_invalid_or_fractional_definitions() {
        for properties in [
            json!({"cron_expression":"0 8 * * *"}),
            json!({"timezone":"UTC"}),
            json!({"retry_limit":1}),
            json!({"cron_expression":"0 25 * * *","timezone":"UTC"}),
            json!({"cron_expression":"*/0 * * * *","timezone":"UTC"}),
            json!({"cron_expression":"0 8 * * *","timezone":"Asia/DoesNotExist"}),
            json!({"cron_expression":"0 8 * * *","timezone":"UTC "}),
            json!({"cron_expression":"0 8 * * *","timezone":"UTC","retry_limit":-1}),
            json!({"cron_expression":"0 8 * * *","timezone":"UTC","retry_limit":1.5}),
            json!({"cron_expression":"0 8 * * *","timezone":"UTC","retry_limit":"1"}),
            json!({"cron_expression":"0 8 * * *","timezone":"UTC","retry_limit":11}),
        ] {
            assert!(validate(&properties).is_err(), "accepted {properties}");
        }
    }
}
