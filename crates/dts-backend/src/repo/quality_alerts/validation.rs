use super::*;
use std::collections::BTreeSet;

fn bad(text: &str) -> BackendError {
    BackendError::ConstraintViolation(text.into())
}
pub fn identifier(value: &str) -> Result<(), BackendError> {
    if value.is_empty()
        || value.len() > 128
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(bad("质量告警标识无效"));
    }
    Ok(())
}
pub fn pagination(limit: u32, offset: u32) -> Result<(), BackendError> {
    if !(1..=100).contains(&limit) || offset > 1_000_000 {
        return Err(bad("质量告警分页范围无效"));
    }
    Ok(())
}
pub fn filter(filter: &QualityAlertFilter) -> Result<(), BackendError> {
    identifier(&filter.tenant_id)?;
    identifier(&filter.monitor_id)?;
    pagination(filter.limit, filter.offset)?;
    if filter
        .status
        .as_deref()
        .is_some_and(|status| !matches!(status, "open" | "acknowledged" | "resolved"))
    {
        return Err(bad("告警状态无效"));
    }
    Ok(())
}
pub fn evaluation_semantics(
    tenant: &str,
    actor: &str,
    monitor: &str,
    version: i64,
    key: &str,
) -> Result<Value, BackendError> {
    for value in [tenant, actor, monitor, key] {
        identifier(value)?;
    }
    if version < 1 {
        return Err(bad("监控版本无效"));
    }
    Ok(
        serde_json::json!({"action":"evaluate","actor_id":actor,"monitor_id":monitor,"expected_version":version}),
    )
}
pub fn ack_semantics(params: &AcknowledgeQualityAlertParams) -> Result<Value, BackendError> {
    for value in [
        &params.tenant_id,
        &params.actor_id,
        &params.alert_id,
        &params.idempotency_key,
    ] {
        identifier(value)?;
    }
    if params.expected_version < 1
        || params.reason.trim().is_empty()
        || params.reason.len() > 2000
        || params.reason.chars().any(char::is_control)
    {
        return Err(bad("确认版本或处理说明无效"));
    }
    Ok(
        serde_json::json!({"action":"acknowledge","actor_id":params.actor_id,"alert_id":params.alert_id,"expected_version":params.expected_version,"reason":params.reason}),
    )
}
pub fn evaluation(params: &RecordQualityEvaluationParams) -> Result<Value, BackendError> {
    let observation = &params.observation;
    let semantics = evaluation_semantics(
        &observation.tenant_id,
        &observation.actor_id,
        &observation.monitor_id,
        observation.monitor_version,
        &params.idempotency_key,
    )?;
    identifier(&observation.id)?;
    identifier(&observation.object_id)?;
    if observation.period_start >= observation.period_end
        || observation.period_end - observation.period_start > chrono::Duration::days(366)
        || !matches!(
            observation.state.as_str(),
            "threshold_exceeded" | "within_threshold" | "unavailable"
        )
    {
        return Err(bad("观察窗口或实际评估状态无效"));
    }
    if observation.reason.is_empty()
        || observation.reason.len() > 128
        || observation.reason.chars().any(char::is_control)
    {
        return Err(bad("观察原因无效"));
    }
    let exceeded = observation.summary["threshold_exceeded"].as_bool();
    if (observation.state == "threshold_exceeded" && exceeded != Some(true))
        || (observation.state == "within_threshold" && exceeded != Some(false))
        || (observation.state == "unavailable" && exceeded.is_some())
    {
        return Err(bad("观察状态与真实阈值结果不一致"));
    }
    if observation.summary.to_string().len() > 16384
        || serde_json::to_vec(&observation.snapshot)
            .map_err(|_| bad("观察快照无法编码"))?
            .len()
            > 524288
        || observation.snapshot.jobs.len() > 512
        || observation.snapshot.resources.len() > 256
    {
        return Err(bad("观察快照超过受控容量，须缩短监控窗口"));
    }
    let mut refs = BTreeSet::new();
    for resource in &observation.snapshot.resources {
        identifier(&resource.id)?;
        if !matches!(
            resource.kind.as_str(),
            "quality_monitor"
                | "quality_object"
                | "quality_rule"
                | "quality_task"
                | "metadata_object"
        ) || resource.version < 1
            || !refs.insert((&resource.kind, &resource.id))
        {
            return Err(bad("冻结质量引用无效或重复"));
        }
    }
    if !observation.snapshot.resources.iter().any(|resource| {
        resource.kind == "quality_monitor"
            && resource.id == observation.monitor_id
            && resource.version == observation.monitor_version
    }) || !observation
        .snapshot
        .resources
        .iter()
        .any(|resource| resource.kind == "quality_object" && resource.id == observation.object_id)
    {
        return Err(bad("监控和对象缺少冻结证明"));
    }
    let mut recipients = BTreeSet::new();
    for id in &params.recipient_ids {
        identifier(id)?;
        if !recipients.insert(id) {
            return Err(bad("告警收件人重复"));
        }
    }
    if params.recipient_ids.len() > 100
        || params.notice_subject.trim().is_empty()
        || params.notice_subject.len() > 512
        || params.notice_subject.chars().any(char::is_control)
    {
        return Err(bad("告警提醒标题或收件人数无效"));
    }
    verify_counts(observation)?;
    Ok(semantics)
}

fn verify_counts(observation: &QualityObservation) -> Result<(), BackendError> {
    let monitor = observation
        .snapshot
        .resources
        .iter()
        .find(|resource| {
            resource.kind == "quality_monitor" && resource.id == observation.monitor_id
        })
        .ok_or_else(|| bad("监控证明缺失"))?;
    let threshold = monitor.properties["failure_threshold"]
        .as_f64()
        .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
        .ok_or_else(|| bad("冻结阈值无效"))?;
    let hours = monitor.properties["window_hours"]
        .as_i64()
        .filter(|hours| (1..=8760).contains(hours))
        .ok_or_else(|| bad("冻结时间窗无效"))?;
    if monitor.properties["object_id"].as_str() != Some(observation.object_id.as_str())
        || observation.period_end - observation.period_start != chrono::Duration::hours(hours)
        || observation.summary["threshold"].as_f64() != Some(threshold)
    {
        return Err(bad("观察与实际监控粒度、窗口或阈值不一致"));
    }
    let (
        mut completed,
        mut failed_runs,
        mut incomplete,
        mut checks,
        mut empty,
        mut errors,
        mut sampled,
        mut checked,
        mut failed,
    ) = (0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
    let mut ids = BTreeSet::new();
    for job in &observation.snapshot.jobs {
        if !ids.insert(&job.id)
            || job.event_time < observation.period_start
            || job.event_time >= observation.period_end
        {
            return Err(bad("观察真实运行身份重复或超出窗口"));
        }
        match job.status.as_str() {
            "succeeded" => completed += 1,
            "failed" | "reconciliation_required" => failed_runs += 1,
            "queued" | "running" | "cancelled" => incomplete += 1,
            _ => return Err(bad("冻结运行状态无效")),
        }
        for node in &job.nodes {
            if node.status == "failed" {
                errors += 1;
            }
            if job.status != "succeeded" {
                continue;
            }
            if node.status != "succeeded" {
                return Err(bad("成功运行缺少成功节点"));
            }
            let result = node
                .result
                .as_ref()
                .ok_or_else(|| bad("成功运行缺少真实计数"))?;
            let count = result["checked_rows"]
                .as_u64()
                .ok_or_else(|| bad("真实校验行数无效"))?;
            let failure = result["failed_rows"]
                .as_u64()
                .filter(|value| *value <= count)
                .ok_or_else(|| bad("真实失败行数无效"))?;
            checked = checked
                .checked_add(count)
                .ok_or_else(|| bad("真实计数溢出"))?;
            failed = failed
                .checked_add(failure)
                .ok_or_else(|| bad("真实计数溢出"))?;
            checks += 1;
            if count == 0 {
                empty += 1;
            }
            if result["evaluation_scope"] == "sample" {
                sampled += 1;
            }
        }
    }
    for (key, value) in [
        ("completed_runs", completed),
        ("failed_runs", failed_runs),
        ("incomplete_runs", incomplete),
        ("successful_checks", checks),
        ("empty_checks", empty),
        ("execution_error_checks", errors),
        ("sampled_checks", sampled),
        ("checked_rows", checked),
        ("failed_rows", failed),
    ] {
        if observation.summary[key].as_u64() != Some(value) {
            return Err(bad("观察计数与真实运行节点不一致"));
        }
    }
    let ratio = if checked == 0 {
        None
    } else {
        Some(failed as f64 / checked as f64)
    };
    if observation.summary["failure_ratio"].as_f64() != ratio {
        return Err(bad("观察失败比例与实际计数不一致"));
    }
    let available = observation.summary["window_complete"] == true
        && completed > 0
        && failed_runs == 0
        && incomplete == 0
        && errors == 0
        && empty == 0
        && checked > 0;
    let actual = if available {
        ratio.map(|ratio| ratio > threshold)
    } else {
        None
    };
    if observation.summary["threshold_exceeded"].as_bool() != actual {
        return Err(bad("失败、空数据和不完整窗口不能伪装阈值通过"));
    }
    let reason = if observation.summary["window_complete"] != true {
        "incomplete_window"
    } else if failed_runs > 0 || errors > 0 {
        "execution_error"
    } else if incomplete > 0 {
        "incomplete_runs"
    } else if completed == 0 {
        "no_results"
    } else if checked == 0 {
        "empty_data"
    } else if empty > 0 {
        "partial_empty_data"
    } else if actual == Some(true) {
        "threshold_exceeded"
    } else if sampled > 0 {
        "sample_within_threshold"
    } else {
        "within_threshold"
    };
    if observation.reason != reason || observation.summary["window_complete"].as_bool().is_none() {
        return Err(bad("观察原因与真实执行状态不一致"));
    }
    Ok(())
}
