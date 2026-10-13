use crate::models::{auth::AuthIdentity, error::AppError};
use dts_backend::{ProductResource, ProductResourceRepo, repo::runtime_configuration::*};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
mod parameters;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationActionRequest {
    pub expected_version: i64,
    pub reason: String,
    pub idempotency_key: String,
}

#[derive(Clone)]
pub struct RuntimeConfigurationService {
    repo: Arc<dyn RuntimeConfigurationRepo>,
    resources: Arc<dyn ProductResourceRepo>,
}
impl RuntimeConfigurationService {
    pub fn new(
        repo: Arc<dyn RuntimeConfigurationRepo>,
        resources: Arc<dyn ProductResourceRepo>,
    ) -> Self {
        Self { repo, resources }
    }
    pub async fn validate(
        &self,
        actor: &AuthIdentity,
        id: &str,
        version: i64,
    ) -> Result<Value, AppError> {
        let config = self.repo.get(&lookup(actor, id)).await?;
        if config.version != version {
            return Err(AppError::Conflict("运行配置版本已变化".into()));
        }
        let source = self.policy_source(actor, &config).await?;
        if let Some(source) = source.as_ref() {
            parameters::validate_values(
                source,
                &validation::policy(&config.properties)?.variables,
            )?;
        }
        ValidatedRuntimePolicy::from_validated(&config, source.as_ref())?;
        Ok(
            json!({"valid":true,"executed":false,"validation_scope":"non_secret_runtime_policy_and_actual_source_parameters"}),
        )
    }
    async fn policy_source(
        &self,
        actor: &AuthIdentity,
        config: &ProductResource,
    ) -> Result<Option<ProductResource>, AppError> {
        let policy = validation::policy(&config.properties)?;
        match (policy.source_kind, policy.source_id) {
            (Some(kind), Some(id)) => Ok(Some(
                self.resources
                    .get(&actor.tenant_id, &kind, &id)
                    .await?
                    .ok_or_else(|| AppError::NotFound("配置绑定的真实来源不存在".into()))?,
            )),
            _ => Ok(None),
        }
    }
    pub async fn publish(
        &self,
        actor: &AuthIdentity,
        id: &str,
        request: ConfigurationActionRequest,
    ) -> Result<RuntimeConfigurationChange, AppError> {
        let config = self.repo.get(&lookup(actor, id)).await?;
        let validation = if config.version == request.expected_version {
            let source = self.policy_source(actor, &config).await?;
            if let Some(source) = source.as_ref() {
                parameters::validate_values(
                    source,
                    &validation::policy(&config.properties)?.variables,
                )?;
            }
            Some(ValidatedRuntimePolicy::from_validated(
                &config,
                source.as_ref(),
            )?)
        } else {
            None
        };
        Ok(self
            .repo
            .publish(&PublishRuntimeConfigurationParams {
                lookup: lookup(actor, id),
                expected_version: request.expected_version,
                reason: request.reason,
                idempotency_key: request.idempotency_key,
                validation,
            })
            .await?)
    }
    pub async fn withdraw(
        &self,
        actor: &AuthIdentity,
        id: &str,
        request: ConfigurationActionRequest,
    ) -> Result<RuntimeConfigurationChange, AppError> {
        Ok(self
            .repo
            .withdraw(&WithdrawRuntimeConfigurationParams {
                lookup: lookup(actor, id),
                expected_version: request.expected_version,
                reason: request.reason,
                idempotency_key: request.idempotency_key,
            })
            .await?)
    }
}

fn lookup(actor: &AuthIdentity, id: &str) -> RuntimeConfigurationLookup {
    RuntimeConfigurationLookup {
        scope: RuntimeConfigurationScope {
            tenant_id: actor.tenant_id.clone(),
            actor_id: actor.sub.clone(),
        },
        configuration_id: id.into(),
    }
}
pub(crate) fn source_lookup(
    actor: &AuthIdentity,
    source: &ProductResource,
    access: RuntimeSourceAccess,
) -> RuntimeSourceLookup {
    RuntimeSourceLookup {
        scope: RuntimeConfigurationScope {
            tenant_id: actor.tenant_id.clone(),
            actor_id: actor.sub.clone(),
        },
        source_kind: source.kind.clone(),
        source_id: source.id.clone(),
        source_version: source.version,
        access,
    }
}
pub(crate) fn merged_values(
    source: &ProductResource,
    configuration: Option<&ProductResource>,
    overrides: &Value,
) -> Result<Value, AppError> {
    let mut values = if let Some(config) = configuration {
        validation::policy(&config.properties)?
            .variables
            .as_object()
            .cloned()
            .ok_or_else(|| AppError::BadRequest("配置变量无效".into()))?
    } else {
        serde_json::Map::new()
    };
    values.extend(
        overrides
            .as_object()
            .cloned()
            .ok_or_else(|| AppError::BadRequest("运行参数必须是对象".into()))?,
    );
    let values = Value::Object(values);
    parameters::validate_values(source, &values)?;
    Ok(values)
}
