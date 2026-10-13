use super::{
    QualityCheckResult, QualityCheckStatus, QualityNode, QualityReport, QualityResultPage,
    QualityResultRecord, bad,
};
use crate::models::error::AppError;
use chrono::{DateTime, Utc};
use dts_backend::repo::product_job::*;

pub fn results_from_jobs(
    tenant: &str,
    jobs: &[ProductJob],
    total: i64,
) -> Result<QualityResultPage, AppError> {
    let mut data = Vec::new();
    for job in jobs {
        if job.tenant_id != tenant
            || job.source_kind != "quality_task"
            || job.action != "execute_quality"
        {
            return Err(AppError::Internal(
                "质量运行列表混入不属于当前租户或执行器的记录".into(),
            ));
        }
        for node in &job.nodes {
            data.push(record(job, node)?);
        }
    }
    Ok(QualityResultPage {
        data,
        total_jobs: total,
        jobs_scanned: jobs.len(),
    })
}

fn record(job: &ProductJob, node: &ProductJobNode) -> Result<QualityResultRecord, AppError> {
    let definition: QualityNode = serde_json::from_value(node.definition.clone())
        .map_err(|_| AppError::Internal("持久化质量节点格式无效".into()))?;
    let (status, checked_rows, failed_rows, failure_ratio, error) = match node.status {
        ProductJobNodeStatus::Succeeded => {
            let value = node
                .result
                .clone()
                .ok_or_else(|| AppError::Internal("成功质量节点缺少真实结果".into()))?;
            let result: QualityCheckResult = serde_json::from_value(value)
                .map_err(|_| AppError::Internal("持久化质量计数格式无效".into()))?;
            let status = if result.checked_rows == 0 {
                QualityCheckStatus::EmptyData
            } else if result.failed_rows == 0 {
                QualityCheckStatus::Passed
            } else {
                QualityCheckStatus::Failed
            };
            let ratio = if result.checked_rows == 0 {
                None
            } else {
                Some(result.failed_rows as f64 / result.checked_rows as f64)
            };
            if result.rule_id != definition.rule_id
                || result.rule_version != definition.rule_version
                || result.quality_object_id != definition.quality_object_id
                || result.metadata_object_id != definition.metadata_object_id
                || result.failed_rows > result.checked_rows
                || result.status != status
                || result.failure_ratio != ratio
                || result.evaluation_scope
                    != if definition.sample_limit.is_some() {
                        "sample"
                    } else {
                        "full_table"
                    }
                || definition
                    .sample_limit
                    .is_some_and(|limit| result.checked_rows > u64::from(limit))
            {
                return Err(AppError::Internal(
                    "持久化质量结果身份、范围或计数无法验证".into(),
                ));
            }
            (
                status,
                Some(result.checked_rows),
                Some(result.failed_rows),
                ratio,
                None,
            )
        },
        ProductJobNodeStatus::Failed => (
            QualityCheckStatus::ExecutionError,
            None,
            None,
            None,
            Some(
                node.error
                    .clone()
                    .unwrap_or_else(|| "质量查询执行失败".into()),
            ),
        ),
        _ => (
            QualityCheckStatus::NotCompleted,
            None,
            None,
            None,
            node.error.clone(),
        ),
    };
    Ok(QualityResultRecord {
        job_id: job.id.clone(),
        task_id: job.source_id.clone(),
        task_version: job.source_version,
        node_id: node.id.clone(),
        rule_id: definition.rule_id,
        started_at: node.started_at,
        finished_at: node.finished_at,
        status,
        checked_rows,
        failed_rows,
        failure_ratio,
        error,
        evaluation_scope: if definition.sample_limit.is_some() {
            "sample"
        } else {
            "full_table"
        }
        .into(),
    })
}

#[allow(clippy::too_many_arguments)]
pub fn report_from_jobs(
    tenant: &str,
    jobs: &[ProductJob],
    total: i64,
    task_id: Option<String>,
    object_id: Option<String>,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    threshold: Option<f64>,
) -> Result<QualityReport, AppError> {
    if start >= end || end - start > chrono::Duration::days(366) {
        return Err(bad("质量报告窗口必须是有效的开始/截止时间且不超过366天"));
    }
    if threshold
        .is_some_and(|threshold| !threshold.is_finite() || !(0.0..=1.0).contains(&threshold))
    {
        return Err(bad("报告阈值必须是0至1有限数"));
    }
    let mut report = QualityReport {
        task_id: task_id.clone(),
        quality_object_id: object_id.clone(),
        period_start: start,
        period_end: end,
        completed_runs: 0,
        failed_runs: 0,
        incomplete_runs: 0,
        successful_checks: 0,
        empty_checks: 0,
        execution_error_checks: 0,
        sampled_checks: 0,
        checked_rows: 0,
        failed_rows: 0,
        failure_ratio: None,
        counting_unit: "row_rule_checks".into(),
        status: "no_results".into(),
        threshold,
        threshold_exceeded: None,
        window_complete: jobs.len() as i64 >= total,
        jobs_scanned: jobs.len(),
        total_jobs: total,
    };
    for job in jobs {
        if job.tenant_id != tenant
            || job.source_kind != "quality_task"
            || job.action != "execute_quality"
        {
            return Err(AppError::Internal("质量报告记录不属于当前执行域".into()));
        }
        if task_id.as_ref().is_some_and(|task| task != &job.source_id) {
            continue;
        }
        let event_time = job.finished_at.unwrap_or(job.updated_at);
        if event_time < start || event_time >= end {
            continue;
        }
        if object_id
            .as_ref()
            .is_some_and(|id| job.definition_snapshot["object"]["id"].as_str() != Some(id.as_str()))
        {
            continue;
        }
        match job.status {
            ProductJobStatus::Succeeded => report.completed_runs += 1,
            ProductJobStatus::Failed | ProductJobStatus::ReconciliationRequired => {
                report.failed_runs += 1
            },
            _ => report.incomplete_runs += 1,
        }
        for node in &job.nodes {
            let record = record(job, node)?;
            if record.status == QualityCheckStatus::ExecutionError {
                report.execution_error_checks += 1;
            }
            if job.status != ProductJobStatus::Succeeded {
                continue;
            }
            if node.status != ProductJobNodeStatus::Succeeded {
                return Err(AppError::Internal(
                    "成功质量运行包含未成功执行的节点".into(),
                ));
            }
            report.successful_checks += 1;
            if record.status == QualityCheckStatus::EmptyData {
                report.empty_checks += 1;
            }
            if record.evaluation_scope == "sample" {
                report.sampled_checks += 1;
            }
            report.checked_rows = report
                .checked_rows
                .checked_add(
                    record
                        .checked_rows
                        .ok_or_else(|| bad("成功检查缺少实际校验行数"))?,
                )
                .ok_or_else(|| bad("报告计数超出整数范围"))?;
            report.failed_rows = report
                .failed_rows
                .checked_add(
                    record
                        .failed_rows
                        .ok_or_else(|| bad("成功检查缺少实际失败行数"))?,
                )
                .ok_or_else(|| bad("报告计数超出整数范围"))?;
        }
    }
    report.failure_ratio = if report.checked_rows == 0 {
        None
    } else {
        Some(report.failed_rows as f64 / report.checked_rows as f64)
    };
    if report.window_complete
        && report.failed_runs == 0
        && report.incomplete_runs == 0
        && report.execution_error_checks == 0
        && report.empty_checks == 0
    {
        report.threshold_exceeded = threshold
            .zip(report.failure_ratio)
            .map(|(threshold, ratio)| ratio > threshold);
    }
    report.status = if !report.window_complete {
        "incomplete_window"
    } else if report.failed_runs > 0 || report.execution_error_checks > 0 {
        "execution_error"
    } else if report.incomplete_runs > 0 {
        "incomplete_runs"
    } else if report.completed_runs == 0 {
        "no_results"
    } else if report.checked_rows == 0 {
        "empty_data"
    } else if report.empty_checks > 0 {
        "partial_empty_data"
    } else if report.threshold_exceeded == Some(true) {
        "threshold_exceeded"
    } else if report.threshold_exceeded == Some(false) {
        if report.sampled_checks > 0 {
            "sample_within_threshold"
        } else {
            "within_threshold"
        }
    } else if report.failed_rows > 0 {
        "failed"
    } else if report.sampled_checks > 0 {
        "sample_passed"
    } else {
        "passed"
    }
    .into();
    Ok(report)
}
