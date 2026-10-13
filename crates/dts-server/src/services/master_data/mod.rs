// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use dts_backend::{
    ProductResource, ProductResourceRepo, ResourceReference, ResourceWrite,
    repo::master_data::validation, repo::master_data::*,
};
use serde_json::{Value, json};

use crate::{
    models::{auth::AuthIdentity, error::AppError},
    services::product::{ResourceAction, ResourceInput, ResourceUpdate},
};

#[derive(Clone)]
pub struct MasterDataService {
    repo: Arc<dyn MasterDataRepo>,
    resources: Arc<dyn ProductResourceRepo>,
}

impl MasterDataService {
    pub fn new(repo: Arc<dyn MasterDataRepo>, resources: Arc<dyn ProductResourceRepo>) -> Self {
        Self { repo, resources }
    }
    pub fn handles(kind: &str) -> bool {
        matches!(
            kind,
            "master_data_model" | "master_code_rule" | "master_data_record"
        )
    }

    pub async fn create(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        input: ResourceInput,
    ) -> Result<ProductResource, AppError> {
        let params = self.params(actor, kind, uuid::Uuid::new_v4().to_string(), None, input)?;
        self.save(kind, &params).await
    }

    pub async fn update(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        input: ResourceUpdate,
    ) -> Result<ProductResource, AppError> {
        let params = self.params(
            actor,
            kind,
            id.into(),
            Some(input.expected_version),
            ResourceInput {
                code: input.code,
                name: input.name,
                description: input.description,
                parent_id: input.parent_id,
                properties: input.properties,
            },
        )?;
        self.save(kind, &params).await
    }

    pub async fn delete(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        version: i64,
    ) -> Result<(), AppError> {
        if !Self::handles(kind) {
            return Err(AppError::BadRequest("非主数据模块".into()));
        }
        Ok(self
            .repo
            .delete_draft(&DeleteMasterResourceParams {
                scope: scope(actor, id, version),
                kind: kind.into(),
            })
            .await?)
    }

    async fn save(
        &self,
        kind: &str,
        params: &SaveMasterResourceParams,
    ) -> Result<ProductResource, AppError> {
        Ok(match kind {
            "master_data_model" => self.repo.save_model(params).await?,
            "master_code_rule" => self.repo.save_rule(params).await?,
            "master_data_record" => self.repo.save_record(params).await?,
            _ => return Err(AppError::BadRequest("非主数据模块".into())),
        })
    }

    fn params(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: String,
        version: Option<i64>,
        input: ResourceInput,
    ) -> Result<SaveMasterResourceParams, AppError> {
        let code = input.code.trim().to_owned();
        let name = input.name.trim().to_owned();
        if code.is_empty()
            || !code.as_bytes()[0].is_ascii_alphabetic() && !code.starts_with('_')
            || !code.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
            || code.len() > 128
            || name.is_empty()
            || name.chars().count() > 128
            || name.chars().any(char::is_control)
            || input.parent_id.is_some()
        {
            return Err(AppError::BadRequest("主数据编码、名称或父级无效".into()));
        }
        if input
            .description
            .as_ref()
            .is_some_and(|value| value.len() > 8192)
        {
            return Err(AppError::BadRequest("说明过长".into()));
        }
        if serde_json::to_string(&input.properties)
            .map_err(|error| AppError::Internal(error.to_string()))?
            .len()
            > 256 * 1024
        {
            return Err(AppError::BadRequest("主数据属性超过256KiB".into()));
        }
        let allowed: &[&str] = match kind {
            "master_data_model" => &["columns", "key_fields", "owner_id"],
            "master_code_rule" => &["model_id", "prefix", "width", "start_value"],
            "master_data_record" => &["model_id", "business_key", "values"],
            _ => return Err(AppError::BadRequest("非主数据模块".into())),
        };
        if input
            .properties
            .as_object()
            .is_none_or(|values| values.keys().any(|key| !allowed.contains(&key.as_str())))
        {
            return Err(AppError::BadRequest("主数据属性包含未支持字段".into()));
        }
        let mut references = vec![];
        if kind == "master_data_model" {
            validation::schema(&input.properties)?;
            if input
                .properties
                .get("owner_id")
                .is_some_and(|value| value.as_str().is_none_or(|text| text.len() > 128))
            {
                return Err(AppError::BadRequest(
                    "负责人必须是当前租户实际用户ID".into(),
                ));
            }
            if let Some(owner) = input
                .properties
                .get("owner_id")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
            {
                references.push(ResourceReference {
                    field: "owner_id".into(),
                    target_kind: "user".into(),
                    target_id: owner.into(),
                });
            }
        } else {
            let model = validation::text(&input.properties, "model_id", 128)?;
            references.push(ResourceReference {
                field: "model_id".into(),
                target_kind: "master_data_model".into(),
                target_id: model.into(),
            });
        }
        Ok(SaveMasterResourceParams {
            tenant_id: actor.tenant_id.clone(),
            id,
            actor_id: actor.sub.clone(),
            expected_version: version,
            fields: ResourceWrite {
                code,
                name,
                description: input.description,
                parent_id: None,
                status: "draft".into(),
                properties: input.properties,
                references,
            },
        })
    }

    pub async fn action(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        action: &str,
        input: ResourceAction,
    ) -> Result<Value, AppError> {
        let record = self
            .resources
            .get(&actor.tenant_id, kind, id)
            .await?
            .ok_or_else(|| AppError::NotFound("主数据对象不存在".into()))?;
        if record.version != input.expected_version {
            return Err(AppError::Conflict("主数据对象已经变更".into()));
        }
        let binding = scope(actor, id, input.expected_version);
        let result = match (kind, action) {
            ("master_data_model", "publish") => {
                let publication = self.repo.publish_model(&binding).await?;
                return Ok(
                    json!({"resource":publication.resource,"result":{"model_version":publication.model_version,"schema_snapshot":publication.schema_snapshot,
                    "physical_published":publication.physical_published,"publication_scope":"control_plane_schema"}}),
                );
            },
            ("master_code_rule", "allocate") => {
                let key = input
                    .payload
                    .get("idempotency_key")
                    .and_then(Value::as_str)
                    .ok_or_else(|| AppError::BadRequest("编码分配须带真实幂等键".into()))?;
                serde_json::to_value(
                    self.repo
                        .allocate_code(&MasterAllocateParams {
                            scope: binding,
                            idempotency_key: key.into(),
                        })
                        .await?,
                )
                .map_err(|error| AppError::Internal(error.to_string()))?
            },
            ("master_data_record", "submit" | "approve" | "reject") => {
                let action = match action {
                    "submit" => MasterRecordAction::Submit,
                    "approve" => MasterRecordAction::Approve,
                    _ => MasterRecordAction::Reject,
                };
                let reason = input
                    .payload
                    .get("comment")
                    .or_else(|| input.payload.get("reason"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                let saved = self
                    .repo
                    .transition_record(&MasterRecordActionParams {
                        scope: binding,
                        action,
                        reason,
                    })
                    .await?;
                return Ok(
                    json!({"resource":saved,"result":{"state":saved.status,"physical_published":false}}),
                );
            },
            (_, "validate") => {
                match kind {
                    "master_data_model" => validation::schema(&record.properties)?,
                    "master_code_rule" => {
                        validation::rule(&record.properties)?;
                    },
                    "master_data_record" => {
                        let model = validation::text(&record.properties, "model_id", 128)?;
                        let model = self
                            .resources
                            .get(&actor.tenant_id, "master_data_model", model)
                            .await?
                            .ok_or_else(|| AppError::BadRequest("主数据模型不存在".into()))?;
                        if model.status != "published" {
                            return Err(AppError::Conflict("主数据模型尚未冻结发布".into()));
                        }
                        validation::record(&model.properties, &record.properties)?;
                    },
                    _ => return Err(AppError::BadRequest("不支持的主数据类型".into())),
                };
                json!({"valid":true,"validation_scope":"control_plane_definition","physical_published":false})
            },
            _ => return Err(AppError::BadRequest("不支持的主数据动作".into())),
        };
        Ok(json!({"resource":record,"result":result}))
    }
}

fn scope(actor: &AuthIdentity, id: &str, version: i64) -> MasterScope {
    MasterScope {
        tenant_id: actor.tenant_id.clone(),
        id: id.into(),
        actor_id: actor.sub.clone(),
        expected_version: version,
    }
}
