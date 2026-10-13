use super::{SqliteStandardGovernanceRepo, authority, dependencies, storage::*};
use dts_backend::{BackendError, repo::standard_governance::*};

pub(super) async fn transition(
    repo: &SqliteStandardGovernanceRepo,
    p: &StandardTransitionParams,
) -> Result<StandardTransitionResult, BackendError> {
    let scope = &p.scope;
    kind(&scope.kind)?;
    text(&scope.id)?;
    version(scope.expected_version)?;
    let mut tx = repo.begin_write(&scope.tenant_id).await?;
    authority::require(
        &mut tx,
        &scope.tenant_id,
        &scope.actor_id,
        &["standards:write", "standards:execute"],
    )
    .await?;
    let before = resource(&mut tx, &scope.tenant_id, &scope.kind, &scope.id).await?;
    if before.version != scope.expected_version {
        return Err(conflict("标准定义版本已改变"));
    }
    let (resource, revision) = match &p.command {
        StandardCommand::Publish {
            expected_properties,
            normalized_properties,
            children,
        } => {
            if before.status != "draft" || before.properties != *expected_properties {
                return Err(conflict("只有当前未修改草稿可发布标准修订"));
            }
            if !normalized_properties.is_object() {
                return Err(invalid("标准定义必须是结构化对象"));
            }
            dependencies::validate_children(normalized_properties, &scope.kind, children)?;
            dependencies::active(&mut tx, &scope.tenant_id, children).await?;
            let resource = write(
                &mut tx,
                &before,
                &scope.actor_id,
                normalized_properties.clone(),
                "published",
            )
            .await?;
            let revision = StandardRevision {
                tenant_id: scope.tenant_id.clone(),
                kind: scope.kind.clone(),
                id: scope.id.clone(),
                revision_version: resource.version,
                code: resource.code.clone(),
                name: resource.name.clone(),
                properties: resource.properties.clone(),
                children: children.clone(),
                definition_hash: revision_hash(
                    &scope.kind,
                    &resource.code,
                    &resource.name,
                    &resource.properties,
                    children,
                )?,
                state: "active".into(),
                state_version: 1,
                published_by: scope.actor_id.clone(),
                published_at: resource.updated_at,
                withdrawn_by: None,
                withdrawn_at: None,
                withdrawal_reason: None,
            };
            sqlx::query("INSERT INTO standard_revision(tenant_id,kind,id,revision_version,definition_hash,payload,published_by,state,state_version) VALUES(?1,?2,?3,?4,?5,?6,?7,'active',1)").bind(&scope.tenant_id).bind(&scope.kind).bind(&scope.id).bind(revision.revision_version).bind(&revision.definition_hash).bind(encode(&revision)?).bind(&scope.actor_id).execute(&mut *tx).await.map_err(error)?;
            for (index, child) in children.iter().enumerate() {
                sqlx::query("INSERT INTO standard_revision_child(tenant_id,parent_kind,parent_id,parent_version,position,child_kind,child_id,child_version) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)").bind(&scope.tenant_id).bind(&scope.kind).bind(&scope.id).bind(revision.revision_version).bind(index as i64).bind(&child.kind).bind(&child.id).bind(child.revision_version).execute(&mut *tx).await.map_err(error)?;
            }
            (resource, Some(revision))
        },
        StandardCommand::Revise => {
            if before.status != "published" {
                return Err(conflict("只有已发布标准可创建新草稿"));
            }
            let revision = revision(
                &mut tx,
                &scope.tenant_id,
                &scope.kind,
                &scope.id,
                before.version,
            )
            .await?
            .filter(|revision| {
                revision.state == "active" && revision.properties == before.properties
            })
            .ok_or_else(|| conflict("标准当前发布修订不存在或已失效"))?;
            (
                write(
                    &mut tx,
                    &before,
                    &scope.actor_id,
                    before.properties.clone(),
                    "draft",
                )
                .await?,
                Some(revision),
            )
        },
        StandardCommand::Withdraw {
            revision_version,
            expected_state_version,
            reason,
        } => {
            version(*revision_version)?;
            version(*expected_state_version)?;
            if reason.trim().is_empty()
                || reason.len() > 2000
                || reason.chars().any(char::is_control)
            {
                return Err(invalid("撤回说明必须是1至2000字符的有效文本"));
            }
            let mut revision = revision(
                &mut tx,
                &scope.tenant_id,
                &scope.kind,
                &scope.id,
                *revision_version,
            )
            .await?
            .ok_or_else(|| BackendError::NotFound("标准修订不存在".into()))?;
            if revision.state != "active" || revision.state_version != *expected_state_version {
                return Err(conflict("标准修订状态版本已改变"));
            }
            let now = chrono::Utc::now();
            let changed=sqlx::query("UPDATE standard_revision SET state='withdrawn',state_version=state_version+1,withdrawn_by=?1,withdrawn_at=?2,withdrawal_reason=?3 WHERE tenant_id=?4 AND kind=?5 AND id=?6 AND revision_version=?7 AND state='active' AND state_version=?8").bind(&scope.actor_id).bind(now.to_rfc3339()).bind(reason).bind(&scope.tenant_id).bind(&scope.kind).bind(&scope.id).bind(*revision_version).bind(*expected_state_version).execute(&mut *tx).await.map_err(error)?.rows_affected();
            if changed != 1 {
                return Err(conflict("标准修订状态并发变更"));
            }
            revision.state = "withdrawn".into();
            revision.state_version += 1;
            revision.withdrawn_by = Some(scope.actor_id.clone());
            revision.withdrawn_at = Some(now);
            revision.withdrawal_reason = Some(reason.clone());
            let resource = if before.status == "published" && before.version == *revision_version {
                write(
                    &mut tx,
                    &before,
                    &scope.actor_id,
                    before.properties.clone(),
                    "draft",
                )
                .await?
            } else {
                before
            };
            audit(&mut tx, scope, "withdraw", Some(*revision_version)).await?;
            (resource, Some(revision))
        },
    };
    audit(
        &mut tx,
        scope,
        "standard_transition",
        revision.as_ref().map(|revision| revision.revision_version),
    )
    .await?;
    tx.commit().await.map_err(error)?;
    Ok(StandardTransitionResult { resource, revision })
}
pub(super) async fn audit(
    conn: &mut sqlx::SqliteConnection,
    scope: &StandardScope,
    action: &str,
    revision: Option<i64>,
) -> Result<(), BackendError> {
    sqlx::query("INSERT INTO product_resource_audit(id,tenant_id,actor_id,resource_kind,resource_id,operation,summary,created_at) VALUES(?1,?2,?3,?4,?5,'update',?6,?7)").bind(uuid::Uuid::new_v4().to_string()).bind(&scope.tenant_id).bind(&scope.actor_id).bind(&scope.kind).bind(&scope.id).bind(serde_json::json!({"action":action,"revision_version":revision}).to_string()).bind(chrono::Utc::now().to_rfc3339()).execute(conn).await.map_err(error)?;
    Ok(())
}
