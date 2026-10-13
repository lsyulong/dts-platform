use super::{
    MaskingService, bad, denied,
    policy::{group_definition, rule_definition},
};
use crate::models::{auth::AuthIdentity, error::AppError};
use dts_backend::{ProductResource, ResourceReference, ResourceWrite, UpdateResourceParams};
use serde::Deserialize;
use serde_json::{Value, json};

impl MaskingService {
    pub async fn validate_definition(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        properties: &Value,
    ) -> Result<Value, AppError> {
        self.active_user(&actor.tenant_id, &actor.sub).await?;
        match kind {
            "masking_rule" => {
                let rule = rule_definition(properties)?;
                let target = self.bind_rule(&actor.tenant_id, &rule, false).await?;
                let column = target
                    .columns
                    .iter()
                    .find(|column| column.name == rule.column_name)
                    .ok_or_else(|| bad("真实脱敏字段不存在"))?;
                Ok(
                    json!({"valid":true,"metadata_id":target.metadata_id,"metadata_version":target.binding.object_version,
                    "column_name":column.name,"column_data_type":column.data_type,"activation_required":true}),
                )
            },
            "user_group" => {
                let group = group_definition(properties)?;
                for id in &group.member_ids {
                    self.active_user(&actor.tenant_id, id).await?;
                }
                if let Some(id) = &group.masking_rule_id {
                    self.resources
                        .get(&actor.tenant_id, "masking_rule", id)
                        .await?
                        .ok_or_else(|| bad("脱敏规则不属于当前租户或不存在"))?;
                }
                Ok(
                    json!({"valid":true,"member_count":group.member_ids.len(),"activation_required":true}),
                )
            },
            _ => Err(bad("当前对象不属于脱敏配置领域")),
        }
    }

    pub async fn validate_saved(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
    ) -> Result<Value, AppError> {
        let resource = self.resource(actor, kind, id).await?;
        self.validate_definition(actor, kind, &resource.properties)
            .await
    }

    pub async fn publish(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        expected_version: i64,
    ) -> Result<ProductResource, AppError> {
        self.manager(actor).await?;
        let resource = self.resource(actor, kind, id).await?;
        if !matches!(resource.status.as_str(), "draft" | "inactive") {
            return Err(bad("仅草稿或停用的脱敏配置可发布"));
        }
        self.validate_definition(actor, kind, &resource.properties)
            .await?;
        let mut properties = resource.properties.clone();
        match kind {
            "masking_rule" => {
                let rule = rule_definition(&properties)?;
                let target = self.bind_rule(&actor.tenant_id, &rule, false).await?;
                let column = target
                    .columns
                    .iter()
                    .find(|column| column.name == rule.column_name)
                    .ok_or_else(|| bad("真实脱敏字段不存在"))?;
                properties["metadata_version"] = json!(target.binding.object_version);
                properties["column_data_type"] = json!(column.data_type);
                properties["source_fingerprint"] = json!(target.binding.source_fingerprint);
            },
            "user_group" => {
                let group = group_definition(&properties)?;
                if let Some(rule_id) = group.masking_rule_id {
                    let rule = self
                        .resources
                        .get(&actor.tenant_id, "masking_rule", &rule_id)
                        .await?
                        .filter(|rule| rule.status == "published")
                        .ok_or_else(|| bad("发布用户组前必须先发布其脱敏规则"))?;
                    self.bind_rule(&actor.tenant_id, &rule_definition(&rule.properties)?, true)
                        .await?;
                }
            },
            _ => return Err(bad("当前对象不属于脱敏配置领域")),
        }
        self.write(actor, &resource, expected_version, "published", properties)
            .await
    }

    pub async fn withdraw(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        expected_version: i64,
    ) -> Result<ProductResource, AppError> {
        self.manager(actor).await?;
        let resource = self.resource(actor, kind, id).await?;
        if resource.status != "published" {
            return Err(bad("仅已发布的脱敏配置可停用"));
        }
        self.write(
            actor,
            &resource,
            expected_version,
            "inactive",
            resource.properties.clone(),
        )
        .await
    }

    pub async fn assign_members(
        &self,
        actor: &AuthIdentity,
        id: &str,
        expected_version: i64,
        payload: &Value,
    ) -> Result<ProductResource, AppError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Members {
            member_ids: Vec<String>,
        }
        let members: Members = serde_json::from_value(payload.clone())
            .map_err(|_| bad("成员分配只接受member_ids数组"))?;
        self.manager(actor).await?;
        let resource = self.resource(actor, "user_group", id).await?;
        let mut properties = resource.properties.clone();
        properties["member_ids"] = json!(members.member_ids);
        self.validate_definition(actor, "user_group", &properties)
            .await?;
        self.write(actor, &resource, expected_version, "draft", properties)
            .await
    }

    pub async fn preview(
        &self,
        actor: &AuthIdentity,
        id: &str,
        payload: &Value,
    ) -> Result<Value, AppError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Preview {
            sample: Value,
        }
        self.manager(actor).await?;
        let payload: Preview =
            serde_json::from_value(payload.clone()).map_err(|_| bad("预览参数只接受sample"))?;
        if payload.sample.is_array()
            || payload.sample.is_object()
            || payload.sample.to_string().len() > 4096
        {
            return Err(bad("预览只接受不超过4096字节的单个值"));
        }
        let resource = self.resource(actor, "masking_rule", id).await?;
        let definition = rule_definition(&resource.properties)?;
        let target = self.bind_rule(&actor.tenant_id, &definition, false).await?;
        let rule = super::types::BoundRule {
            rule_id: resource.id,
            rule_version: resource.version,
            groups: vec![],
            definition,
            target,
        };
        let masked = self.mask_value(&actor.tenant_id, &rule, &payload.sample)?;
        Ok(
            json!({"masked_value":masked,"sample_persisted":false,"policy_active":resource.status == "published"}),
        )
    }

    async fn manager(&self, actor: &AuthIdentity) -> Result<(), AppError> {
        self.active_user(&actor.tenant_id, &actor.sub).await?;
        let user = self
            .users
            .find_detail_by_id(&actor.sub)
            .await?
            .filter(|user| {
                user.tenant_id == actor.tenant_id
                    && user
                        .permissions
                        .iter()
                        .any(|permission| permission == "privacy:write")
            })
            .ok_or_else(|| denied("脱敏配置动作要求当前租户明确的privacy:write权限"))?;
        let _ = user;
        Ok(())
    }

    async fn resource(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
    ) -> Result<ProductResource, AppError> {
        if !matches!(kind, "masking_rule" | "user_group") {
            return Err(bad("当前对象不属于脱敏配置领域"));
        }
        self.resources
            .get(&actor.tenant_id, kind, id)
            .await?
            .filter(|resource| resource.tenant_id == actor.tenant_id && resource.kind == kind)
            .ok_or_else(|| AppError::NotFound("脱敏配置不存在".into()))
    }

    async fn write(
        &self,
        actor: &AuthIdentity,
        resource: &ProductResource,
        version: i64,
        status: &str,
        properties: Value,
    ) -> Result<ProductResource, AppError> {
        let references = match resource.kind.as_str() {
            "masking_rule" => vec![ResourceReference {
                field: "object_id".into(),
                target_kind: "metadata_object".into(),
                target_id: rule_definition(&properties)?.object_id,
            }],
            "user_group" => {
                let group = group_definition(&properties)?;
                let mut references: Vec<_> = group
                    .member_ids
                    .into_iter()
                    .map(|id| ResourceReference {
                        field: "member_ids".into(),
                        target_kind: "user".into(),
                        target_id: id,
                    })
                    .collect();
                if let Some(id) = group.masking_rule_id {
                    references.push(ResourceReference {
                        field: "masking_rule_id".into(),
                        target_kind: "masking_rule".into(),
                        target_id: id,
                    });
                }
                references
            },
            _ => return Err(bad("当前对象不属于脱敏配置领域")),
        };
        Ok(self
            .resources
            .update(
                &actor.tenant_id,
                &resource.kind,
                &resource.id,
                &UpdateResourceParams {
                    expected_version: version,
                    actor_id: actor.sub.clone(),
                    fields: ResourceWrite {
                        code: resource.code.clone(),
                        name: resource.name.clone(),
                        description: resource.description.clone(),
                        parent_id: None,
                        status: status.into(),
                        properties,
                        references,
                    },
                },
            )
            .await?)
    }
}
