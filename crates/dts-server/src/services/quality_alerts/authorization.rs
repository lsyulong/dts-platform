use super::*;
use crate::services::{
    data_access::{AccessOutcome, AuthorizedQuery},
    data_quality::prepare_claimed,
};
use dts_backend::repo::{
    metadata_catalog::binding_matches,
    product_job::{ProductJobLookup, ProductJobNodeStatus},
};

impl QualityAlertsService {
    pub(super) async fn authorize_monitor(
        &self,
        actor: &AuthIdentity,
        id: &str,
    ) -> Result<(), AppError> {
        let monitor = self.resource(actor, "quality_monitor", id).await?;
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
        let token = self
            .table_entitlement(actor, &metadata.id, metadata.version, &metadata.properties)
            .await?;
        self.finish_read(actor, &[token]).await
    }
    async fn table_entitlement(
        &self,
        actor: &AuthIdentity,
        id: &str,
        version: i64,
        properties: &Value,
    ) -> Result<AuthorizedQuery, AppError> {
        let binding = self
            .metadata
            .verified_binding(&actor.tenant_id, id, version)
            .await?
            .filter(|binding| binding_matches(binding, properties))
            .ok_or_else(|| AppError::Forbidden("质量告警缺少当前真实元数据采集证明".into()))?;
        let quoted = [
            binding.table_catalog,
            binding.schema_name,
            binding.table_name,
        ]
        .iter()
        .map(|value| {
            if value.is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
                return Err(AppError::Forbidden("真实表定位无效".into()));
            }
            Ok(format!("\"{}\"", value.replace('"', "\"\"")))
        })
        .collect::<Result<Vec<_>, AppError>>()?
        .join(".");
        // The ordinary COUNT(*) guard checks all columns. This only authorizes
        // access to persisted aggregate observations; it does not run SQL.
        self.guard
            .authorize(actor, &format!("SELECT COUNT(*) FROM {quoted} LIMIT 1"))
            .await
    }
    pub(super) async fn authorize_observation(
        &self,
        actor: &AuthIdentity,
        observation: &QualityObservation,
    ) -> Result<Vec<AuthorizedQuery>, AppError> {
        if observation.tenant_id != actor.tenant_id {
            return Err(AppError::Forbidden("告警观察不属于当前租户".into()));
        }
        for proof in &observation.snapshot.resources {
            let current = self.resource(actor, &proof.kind, &proof.id).await?;
            if current.version != proof.version || current.properties != proof.properties {
                return Err(AppError::Forbidden(
                    "观察依赖的监控、任务、规则或数据对象已变更；旧计数访问已关闭".into(),
                ));
            }
        }
        let snapshot = &observation.snapshot;
        let metadata = snapshot
            .resources
            .iter()
            .find(|proof| {
                proof.kind == "metadata_object"
                    && proof.id == snapshot.metadata_id
                    && proof.version == snapshot.metadata_version
            })
            .ok_or_else(|| AppError::Internal("告警观察缺少真实元数据证明".into()))?;
        let mut tokens = vec![
            self.table_entitlement(actor, &metadata.id, metadata.version, &metadata.properties)
                .await?,
        ];
        for proof in &snapshot.jobs {
            if proof.status != "succeeded" {
                continue;
            }
            let current = self
                .jobs
                .get(&ProductJobLookup {
                    tenant_id: actor.tenant_id.clone(),
                    job_id: proof.id.clone(),
                    actor_id: None,
                })
                .await?
                .ok_or_else(|| AppError::Forbidden("观察的真实质量运行已不存在".into()))?;
            if super::collect::job_proof(&current)? != *proof {
                return Err(AppError::Forbidden("观察的真实质量运行或结果已改变".into()));
            }
            for (node_id, sql) in prepare_claimed(&current)? {
                if !current.nodes.iter().any(|node| {
                    node.id == node_id
                        && node.status == ProductJobNodeStatus::Succeeded
                        && node.result.is_some()
                }) {
                    return Err(AppError::Forbidden("质量计数缺少真实成功节点".into()));
                }
                tokens.push(
                    self.guard
                        .authorize_quality_result(actor, &current, &node_id, &sql)
                        .await?,
                );
            }
        }
        for token in &tokens {
            self.guard.reauthorize(actor, token).await?;
        }
        Ok(tokens)
    }
    pub(super) async fn finish_read(
        &self,
        actor: &AuthIdentity,
        tokens: &[AuthorizedQuery],
    ) -> Result<(), AppError> {
        self.permission(actor, false).await?;
        for token in tokens {
            if let Err(error) = self.guard.reauthorize(actor, token).await {
                self.guard
                    .record_access(actor, token, AccessOutcome::Denied)
                    .await?;
                return Err(error);
            }
            self.guard
                .record_access(actor, token, AccessOutcome::Succeeded)
                .await?;
        }
        Ok(())
    }
}
