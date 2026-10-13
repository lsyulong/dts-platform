use super::*;
use crate::services::data_quality::{QualitySnapshot, prepare_claimed, report_from_jobs};
use chrono::{Duration, Utc};
use dts_backend::{
    ProductResource,
    repo::{metadata_catalog::binding_matches, product_job::*},
};
use serde_json::json;
use std::collections::BTreeMap;

impl QualityAlertsService {
    pub(super) async fn collect(
        &self,
        actor: &AuthIdentity,
        id: &str,
        request: QualityEvaluateRequest,
    ) -> Result<RecordQualityEvaluationParams, AppError> {
        let monitor = self.resource(actor, "quality_monitor", id).await?;
        if monitor.version != request.expected_version {
            return Err(AppError::Conflict("监控定义版本已经改变".into()));
        }
        validate_definition(&monitor.properties)?;
        let object = self
            .resource(
                actor,
                "quality_object",
                &text(&monitor.properties, "object_id")?,
            )
            .await?;
        let metadata = self
            .resource(
                actor,
                "metadata_object",
                &text(&object.properties, "metadata_object_id")?,
            )
            .await?;
        let binding = self
            .metadata
            .verified_binding(&actor.tenant_id, &metadata.id, metadata.version)
            .await?
            .filter(|binding| binding_matches(binding, &metadata.properties))
            .ok_or_else(|| AppError::Forbidden("质量监控缺少当前真实元数据采集证明".into()))?;
        let threshold = monitor.properties["failure_threshold"]
            .as_f64()
            .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
            .ok_or_else(|| AppError::BadRequest("监控阈值须为0至1有限数".into()))?;
        let hours = monitor.properties["window_hours"]
            .as_i64()
            .filter(|value| (1..=8760).contains(value))
            .ok_or_else(|| AppError::BadRequest("监控窗口须为1至8760小时整数".into()))?;
        let end = Utc::now();
        let start = end
            .checked_sub_signed(Duration::hours(hours))
            .ok_or_else(|| AppError::BadRequest("监控窗口超出时间范围".into()))?;
        let (mut jobs, complete) = self.window_jobs(actor, &object.id, start, end).await?;
        let total = jobs.len();
        let capped = total > 512;
        jobs.truncate(512);
        let mut refs = BTreeMap::new();
        for resource in [&monitor, &object, &metadata] {
            refs.insert(
                (resource.kind.clone(), resource.id.clone()),
                proof(resource),
            );
        }
        for job in &jobs {
            prepare_claimed(job)?;
            let snapshot: QualitySnapshot = serde_json::from_value(job.definition_snapshot.clone())
                .map_err(|_| AppError::Internal("质量运行冻结快照无效".into()))?;
            if snapshot.object.id != object.id || snapshot.metadata.id != metadata.id {
                return Err(AppError::Conflict(
                    "质量运行的对象定位与当前监控不一致".into(),
                ));
            }
            for (kind, source) in [
                ("quality_task", &snapshot.task),
                ("quality_object", &snapshot.object),
                ("metadata_object", &snapshot.metadata),
            ]
            .into_iter()
            .chain(snapshot.rules.iter().map(|source| ("quality_rule", source)))
            {
                let current = self.resource(actor, kind, &source.id).await?;
                if current.version != source.version || current.properties != source.properties {
                    return Err(AppError::Conflict(
                        "窗口中的任务、对象、规则或元数据版本已改变，须重新运行质量任务".into(),
                    ));
                }
                let key = (kind.to_owned(), source.id.clone());
                let current_proof = proof(&current);
                if refs.get(&key).is_some_and(|proof| proof != &current_proof) {
                    return Err(AppError::Conflict("监控窗口含不一致的依赖版本".into()));
                }
                refs.insert(key, current_proof);
            }
        }
        let mut report = report_from_jobs(
            &actor.tenant_id,
            &jobs,
            if complete && !capped {
                jobs.len() as i64
            } else {
                jobs.len() as i64 + 1
            },
            None,
            Some(object.id.clone()),
            start,
            end,
            Some(threshold),
        )?;
        report.window_complete = complete && !capped;
        if !report.window_complete {
            report.status = "incomplete_window".into();
            report.threshold_exceeded = None;
        }
        let state = match report.threshold_exceeded {
            Some(true) => "threshold_exceeded",
            Some(false) => "within_threshold",
            None => "unavailable",
        };
        let recipient_ids = match monitor.properties.get("recipient_ids") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(values)) => values
                .iter()
                .map(|value| {
                    value.as_str().map(str::to_owned).ok_or_else(|| {
                        AppError::BadRequest("告警收件人必须是真实用户标识列表".into())
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
            _ => return Err(AppError::BadRequest("告警收件人格式无效".into())),
        };
        let observation = QualityObservation {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id: actor.tenant_id.clone(),
            monitor_id: monitor.id,
            monitor_version: monitor.version,
            object_id: object.id,
            actor_id: actor.sub.clone(),
            period_start: start,
            period_end: end,
            state: state.into(),
            reason: report.status.clone(),
            summary: json!(report),
            snapshot: QualityObservationSnapshot {
                resources: refs.into_values().collect(),
                metadata_id: metadata.id,
                metadata_version: metadata.version,
                source_fingerprint: binding.source_fingerprint,
                jobs: jobs.iter().map(job_proof).collect::<Result<_, _>>()?,
            },
            created_at: end,
        };
        let params = RecordQualityEvaluationParams {
            observation,
            idempotency_key: request.idempotency_key,
            recipient_ids,
            notice_subject: format!("质量告警 / Quality alert: {}", monitor.name),
        };
        dts_backend::repo::quality_alerts::validation::evaluation(&params)?;
        Ok(params)
    }
    async fn window_jobs(
        &self,
        actor: &AuthIdentity,
        object: &str,
        start: chrono::DateTime<Utc>,
        end: chrono::DateTime<Utc>,
    ) -> Result<(Vec<ProductJob>, bool), AppError> {
        let mut data = Vec::new();
        let mut offset = 0;
        let mut seen = std::collections::BTreeSet::new();
        let mut complete = false;
        let mut evidence_bytes = 0usize;
        while offset < 2000 {
            let page = self
                .jobs
                .list(&ProductJobFilter {
                    tenant_id: actor.tenant_id.clone(),
                    actor_id: None,
                    source_kind: Some("quality_task".into()),
                    source_id: None,
                    status: None,
                    limit: 100,
                    offset,
                })
                .await?;
            let count = page.data.len();
            for job in page.data {
                if !seen.insert(job.id.clone()) {
                    return Err(AppError::Conflict(
                        "质量运行扫描期间分页发生变化，请重试".into(),
                    ));
                }
                let time = job.finished_at.unwrap_or(job.updated_at);
                if time >= start
                    && time < end
                    && job.definition_snapshot["object"]["id"].as_str() == Some(object)
                    && data.len() < 513
                {
                    evidence_bytes = evidence_bytes
                        .checked_add(
                            serde_json::to_vec(&job)
                                .map_err(|_| AppError::Internal("质量运行证据编码失败".into()))?
                                .len(),
                        )
                        .ok_or_else(|| AppError::BadRequest("质量运行证据超过容量".into()))?;
                    if evidence_bytes > 512 * 1024 {
                        return Err(AppError::BadRequest(
                            "监控窗口的真实运行证据超过512KiB，请缩短窗口后重新评估".into(),
                        ));
                    }
                    data.push(job);
                }
            }
            offset += count as u32;
            if offset as i64 >= page.total {
                complete = true;
                break;
            }
            if count == 0 {
                break;
            }
        }
        Ok((data, complete))
    }
}
fn proof(resource: &ProductResource) -> QualityResourceProof {
    QualityResourceProof {
        kind: resource.kind.clone(),
        id: resource.id.clone(),
        version: resource.version,
        properties: resource.properties.clone(),
    }
}
fn enum_text(value: &impl serde::Serialize) -> Result<String, AppError> {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .ok_or_else(|| AppError::Internal("质量运行状态编码无效".into()))
}
pub(super) fn job_proof(job: &ProductJob) -> Result<QualityJobProof, AppError> {
    Ok(QualityJobProof {
        id: job.id.clone(),
        source_id: job.source_id.clone(),
        source_version: job.source_version,
        status: enum_text(&job.status)?,
        updated_at: job.updated_at,
        event_time: job.finished_at.unwrap_or(job.updated_at),
        definition_snapshot: job.definition_snapshot.clone(),
        nodes: job
            .nodes
            .iter()
            .map(|node| {
                Ok(QualityNodeProof {
                    id: node.id.clone(),
                    status: enum_text(&node.status)?,
                    definition: node.definition.clone(),
                    result: node.result.clone(),
                })
            })
            .collect::<Result<_, AppError>>()?,
    })
}
