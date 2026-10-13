use super::super::product_resource::{PgProductResourceRepo, helpers::validate_graph};
use super::storage::{self, Receipt, conflict, error};
use dts_backend::{BackendError, ResourceWrite, repo::development_revisions::*};
use sqlx::PgConnection;

pub(super) async fn restore(
    conn: &mut PgConnection,
    p: &RestoreDevelopmentRevisionParams,
) -> Result<RestoreDevelopmentRevisionResult, BackendError> {
    let s = &p.scope;
    let semantic = validation::semantic(s, p.expected_version, &p.reason, Some(&p.revision_id));
    if let Some(mut result) = storage::replay::<RestoreDevelopmentRevisionResult>(
        &mut *conn,
        s,
        &p.idempotency_key,
        "restore",
        &semantic,
    )
    .await?
    {
        result.replayed = true;
        return Ok(result);
    }
    let before = storage::source(&mut *conn, s).await?;
    if before.version != p.expected_version {
        return Err(conflict("当前定义版本已改变，不能静默恢复"));
    }
    if !matches!(before.status.as_str(), "draft" | "inactive") {
        return Err(conflict("恢复须先将已发布定义撤回，或使用当前草稿"));
    }
    let revision = storage::revision(&mut *conn, s, &p.revision_id).await?;
    if !p.validation.matches(&revision.snapshot)? {
        return Err(conflict("恢复修订未通过当前目录与只读 SQL 校验"));
    }
    let old = &revision.snapshot;
    let fields = ResourceWrite {
        code: old.code.clone(),
        name: old.name.clone(),
        description: old.description.clone(),
        parent_id: old.parent_id.clone(),
        status: "draft".into(),
        properties: old.properties.clone(),
        references: old.references.clone(),
    };
    validate_graph(&mut *conn, &s.tenant_id, &s.kind, &s.resource_id, &fields).await?;
    let next = before
        .version
        .checked_add(1)
        .ok_or_else(|| conflict("定义版本已达到上限"))?;
    let now = storage::now(&mut *conn).await?;
    let affected=sqlx::query("UPDATE product_resource SET code=$1,name=$2,description=$3,parent_id=$4,status='draft',properties=$5::jsonb,version=$6,updated_at=$7::timestamptz WHERE tenant_id=$8 AND kind=$9 AND id=$10 AND version=$11")
        .bind(&fields.code).bind(&fields.name).bind(&fields.description).bind(&fields.parent_id).bind(storage::encode(&fields.properties)?).bind(next).bind(now.to_rfc3339())
        .bind(&s.tenant_id).bind(&s.kind).bind(&s.resource_id).bind(p.expected_version).execute(&mut *conn).await.map_err(error)?.rows_affected();
    if affected != 1 {
        return Err(conflict("当前定义版本已改变"));
    }
    PgProductResourceRepo::write_references(
        &mut *conn,
        &s.tenant_id,
        &s.kind,
        &s.resource_id,
        &fields.references,
    )
    .await?;
    let resource = storage::source(&mut *conn, s).await?;
    let result = RestoreDevelopmentRevisionResult {
        resource,
        revision: revision.summary,
        receipt_id: uuid::Uuid::new_v4().to_string(),
        replayed: false,
        requires_publish: true,
    };
    storage::audit(
        &mut *conn,
        s,
        &result.revision,
        next,
        now,
        "restore_version",
    )
    .await?;
    storage::receipt(
        conn,
        s,
        Receipt {
            id: &result.receipt_id,
            key: &p.idempotency_key,
            action: "restore",
            revision_id: &result.revision.id,
            semantic: &semantic,
            result: &result,
            at: now,
        },
    )
    .await?;
    Ok(result)
}
