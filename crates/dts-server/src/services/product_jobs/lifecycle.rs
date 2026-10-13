use super::*;

impl ProductJobService {
    pub async fn withdraw(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        version: i64,
    ) -> Result<Value, AppError> {
        if !matches!(kind, "sql_document" | "development_workflow") {
            return Err(AppError::BadRequest("当前来源不支持开发定义撤回".into()));
        }
        self.authorize_read(actor, kind).await?;
        self.auth.authorize(actor, "development:write").await?;
        let source = self.source(actor, kind, id, version).await?;
        if source.status != "published" {
            return Err(AppError::Conflict("只能撤回当前已发布定义".into()));
        }
        let updated = self
            .resources
            .update(
                &actor.tenant_id,
                kind,
                id,
                &UpdateResourceParams {
                    expected_version: version,
                    actor_id: actor.sub.clone(),
                    fields: ResourceWrite {
                        code: source.code,
                        name: source.name,
                        description: source.description,
                        parent_id: source.parent_id,
                        status: "inactive".into(),
                        properties: source.properties,
                        references: source.references,
                    },
                },
            )
            .await?;
        Ok(
            json!({"resource":updated,"publication_scope":"withdrawn_read_only_definition","existing_runs_preserved":true,"new_execution_enabled":false}),
        )
    }
}
