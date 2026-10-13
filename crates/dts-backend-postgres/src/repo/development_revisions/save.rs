use super::super::product_resource::helpers::validate_graph;
use super::storage::{self, Receipt, conflict, error};
use dts_backend::ResourceWrite;
use dts_backend::{BackendError, repo::development_revisions::*};
use sqlx::PgConnection;

pub(super) async fn save(
    conn: &mut PgConnection,
    p: &SaveDevelopmentRevisionParams,
) -> Result<SaveDevelopmentRevisionResult, BackendError> {
    let s = &p.scope;
    let semantic = validation::semantic(s, p.expected_version, &p.reason, None);
    if let Some(mut result) = storage::replay::<SaveDevelopmentRevisionResult>(
        &mut *conn,
        s,
        &p.idempotency_key,
        "save",
        &semantic,
    )
    .await?
    {
        result.replayed = true;
        return Ok(result);
    }
    let source = storage::source(&mut *conn, s).await?;
    if source.version != p.expected_version {
        return Err(conflict("定义已变化，不能保存其他版本的修订"));
    }
    if !p
        .validation
        .as_ref()
        .map(|v| v.matches(&source))
        .transpose()?
        .unwrap_or(false)
    {
        return Err(conflict("当前定义未通过相同快照的只读 SQL 校验"));
    }
    let fields = ResourceWrite {
        code: source.code.clone(),
        name: source.name.clone(),
        description: source.description.clone(),
        parent_id: source.parent_id.clone(),
        status: source.status.clone(),
        properties: source.properties.clone(),
        references: source.references.clone(),
    };
    validate_graph(&mut *conn, &s.tenant_id, &s.kind, &s.resource_id, &fields).await?;
    let hash = validation::snapshot_hash(&source)?;
    let now = storage::now(&mut *conn).await?;
    let (revision, created) = match storage::version_revision(&mut *conn, s, source.version).await?
    {
        Some(existing) => {
            if existing.summary.snapshot_hash != hash {
                return Err(conflict("相同资源版本已存在不同快照，不能覆盖历史"));
            }
            (existing.summary, false)
        },
        None => {
            let summary = DevelopmentRevisionSummary {
                id: uuid::Uuid::new_v4().to_string(),
                tenant_id: s.tenant_id.clone(),
                source_kind: s.kind.clone(),
                source_id: s.resource_id.clone(),
                source_version: source.version,
                snapshot_hash: hash,
                saved_by: s.actor_id.clone(),
                saved_at: now,
                reason: p.reason.clone(),
            };
            sqlx::query("INSERT INTO development_revision(id,tenant_id,source_kind,source_id,source_version,snapshot,snapshot_hash,saved_by,saved_at,reason) VALUES($1,$2,$3,$4,$5,$6::jsonb,$7,$8,$9::timestamptz,$10)")
                .bind(&summary.id).bind(&s.tenant_id).bind(&s.kind).bind(&s.resource_id).bind(source.version)
                .bind(storage::encode(&validation::canonical_snapshot(&source)?)?).bind(&summary.snapshot_hash).bind(&s.actor_id).bind(now.to_rfc3339()).bind(&p.reason)
                .execute(&mut *conn).await.map_err(error)?;
            (summary, true)
        },
    };
    let result = SaveDevelopmentRevisionResult {
        revision,
        receipt_id: uuid::Uuid::new_v4().to_string(),
        created,
        replayed: false,
    };
    storage::audit(
        &mut *conn,
        s,
        &result.revision,
        source.version,
        now,
        "save_version",
    )
    .await?;
    storage::receipt(
        conn,
        s,
        Receipt {
            id: &result.receipt_id,
            key: &p.idempotency_key,
            action: "save",
            revision_id: &result.revision.id,
            semantic: &semantic,
            result: &result,
            at: now,
        },
    )
    .await?;
    Ok(result)
}
