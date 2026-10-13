use super::*;
use crate::services::runtime_configuration::{merged_values, source_lookup};
use dts_backend::repo::runtime_configuration::validation as runtime_validation;
use dts_backend::repo::runtime_configuration::*;

impl ProductJobService {
    pub fn with_runtime_configuration(mut self, repo: Arc<dyn RuntimeConfigurationRepo>) -> Self {
        self.runtime_configuration = Some(repo);
        self
    }
    pub(super) async fn runtime_input(
        &self,
        actor: &AuthIdentity,
        source: &ProductResource,
        access: RuntimeSourceAccess,
    ) -> Result<RuntimeSourceInput, AppError> {
        let Some(repo) = self.runtime_configuration.as_ref() else {
            if runtime_validation::configuration_id(source)?.is_some() {
                return Err(AppError::BadRequest(
                    "声明了运行配置，但实际配置消费者未启用".into(),
                ));
            }
            return Ok(RuntimeSourceInput {
                source: source.clone(),
                configuration: None,
                evidence: None,
            });
        };
        Ok(repo
            .current_source(&source_lookup(actor, source, access))
            .await?)
    }
    pub(super) async fn runtime_values(
        &self,
        actor: &AuthIdentity,
        source: &ProductResource,
        overrides: &Value,
    ) -> Result<(Value, Option<RuntimeConfigurationEvidence>), AppError> {
        let input = self
            .runtime_input(actor, source, RuntimeSourceAccess::Execute)
            .await?;
        Ok((
            merged_values(&input.source, input.configuration.as_ref(), overrides)?,
            input.evidence,
        ))
    }
    pub(super) async fn verify_runtime(
        &self,
        actor: &AuthIdentity,
        job: &ProductJob,
        execute: bool,
    ) -> Result<(), AppError> {
        if !matches!(
            job.source_kind.as_str(),
            "sql_document" | "development_workflow"
        ) {
            return Ok(());
        }
        let evidence: Option<RuntimeConfigurationEvidence> = serde_json::from_value(
            job.definition_snapshot
                .get("runtime_configuration")
                .cloned()
                .unwrap_or(Value::Null),
        )
        .map_err(|_| AppError::Conflict("固定运行配置快照无效".into()))?;
        let source = self
            .source(actor, &job.source_kind, &job.source_id, job.source_version)
            .await?;
        let configured = runtime_validation::configuration_id(&source)?.is_some();
        if !configured && evidence.is_none() {
            return Ok(());
        }
        let repo = self
            .runtime_configuration
            .as_ref()
            .ok_or_else(|| AppError::BadRequest("固定运行配置仓储未配置".into()))?;
        repo.verify(&VerifyRuntimeConfigurationParams {
            source: source_lookup(
                actor,
                &source,
                if execute {
                    RuntimeSourceAccess::Execute
                } else {
                    RuntimeSourceAccess::Read
                },
            ),
            evidence,
        })
        .await?;
        Ok(())
    }
}

pub(super) fn query_timeout(job: &ProductJob) -> Result<u64, AppError> {
    let evidence: Option<RuntimeConfigurationEvidence> = serde_json::from_value(
        job.definition_snapshot
            .get("runtime_configuration")
            .cloned()
            .unwrap_or(Value::Null),
    )
    .map_err(|_| AppError::Conflict("运行配置超时证据无效".into()))?;
    match evidence {
        Some(e) if (1..=60).contains(&e.query_timeout_seconds) => {
            Ok(u64::from(e.query_timeout_seconds))
        },
        Some(_) => Err(AppError::Conflict("运行配置超时超过实际引擎硬上限".into())),
        None => Ok(65),
    }
}
