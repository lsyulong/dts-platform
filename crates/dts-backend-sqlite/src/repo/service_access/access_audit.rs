use super::{
    storage::{conflict, error, now},
    subscription,
};
use crate::repo::data_access::{authority, grants, storage as access_storage};
use dts_backend::{
    BackendError,
    repo::{data_access::*, service_access::FinishServiceCallParams},
};
use serde_json::{Value, json};
use sqlx::{Row, SqliteConnection};
use std::collections::BTreeSet;

fn original(p: &FinishServiceCallParams) -> Option<Value> {
    p.access_audit.as_ref().map(|audit| json!({
        "authorization_evidence": audit.evidence,
        "masking_versions": p.masking_versions.as_ref().map(|versions| versions.iter()
            .map(|version| json!({"kind":version.kind,"id":version.id,"version":version.version}))
            .collect::<Vec<_>>()),
        "service_call_id": p.call.id, "api_id": p.call.api_id, "row_count": p.row_count
    }))
}

/// A terminal replay must agree with whether an atomic access receipt was written.
pub(super) async fn replay(
    conn: &mut SqliteConnection,
    p: &FinishServiceCallParams,
) -> Result<(), BackendError> {
    let row = sqlx::query(
        "SELECT tenant_id,actor_id,action,outcome,evidence FROM data_access_audit WHERE id=?1",
    )
    .bind(format!("service_call:{}", p.call.id))
    .fetch_optional(conn)
    .await
    .map_err(error)?;
    match (row, original(p)) {
        (None, None) => Ok(()),
        (Some(row), Some(expected)) => {
            let encoded: String = row.try_get("evidence").map_err(error)?;
            let saved: Value =
                serde_json::from_str(&encoded).map_err(|_| conflict("调用访问回执损坏"))?;
            if row.try_get::<String, _>("tenant_id").map_err(error)? != p.call.tenant_id
                || row.try_get::<String, _>("actor_id").map_err(error)? != p.call.owner_id
                || row.try_get::<String, _>("action").map_err(error)? != "query"
                || row.try_get::<String, _>("outcome").map_err(error)? != "succeeded"
                || expected
                    .as_object()
                    .is_none_or(|fields| fields.iter().any(|(key, value)| saved[key] != *value))
            {
                return Err(conflict("调用访问回执与原始授权语义不一致"));
            }
            Ok(())
        },
        _ => Err(conflict("调用访问审计不能事后添加或省略")),
    }
}

pub(super) async fn write(
    conn: &mut SqliteConnection,
    p: &FinishServiceCallParams,
) -> Result<(), BackendError> {
    let Some(audit) = &p.access_audit else {
        return Ok(());
    };
    privacy(conn, p).await?;
    let api =
        subscription::api(conn, &p.call.tenant_id, &p.call.api_id, p.call.api_version).await?;
    let metadata = &api.properties["publication"]["metadata"];
    let target: DataAccessTarget = serde_json::from_value(json!({
        "metadata_id":metadata["id"],"binding":metadata["binding"],"columns":metadata["columns"]
    }))
    .map_err(|_| conflict("API冻结元数据证明无效"))?;
    let names: Vec<String> = target
        .columns
        .iter()
        .map(|column| column.name.clone())
        .collect();
    validation::target(&target, &p.call.tenant_id, &names)?;
    if metadata["version"] != target.binding.object_version
        || !access_storage::verified(conn, &p.call.tenant_id, &target).await?
    {
        return Err(conflict("API冻结元数据证明已经失效"));
    }
    let authority = authority::get(conn, &p.call.tenant_id, &p.call.owner_id, true).await?;
    if !authority.active || authority.all_data != audit.evidence.all_data {
        return Err(conflict("原查询授权身份已经失效"));
    }
    if audit.evidence.all_data {
        if !audit.evidence.grant_ids.is_empty() {
            return Err(conflict("全量数据许可不能附加其他授权身份"));
        }
    } else {
        verify_grants(conn, p, &target).await?;
    }
    let mut evidence = original(p).ok_or_else(|| conflict("实际访问证据不存在"))?;
    evidence["query_hash"] = json!(audit.evidence.query_hash);
    evidence["grant_ids"] = json!(audit.evidence.grant_ids);
    evidence["metadata_ids"] = json!([target.metadata_id]);
    evidence["columns"] = json!(audit.evidence.columns);
    evidence["all_data"] = json!(audit.evidence.all_data);
    let encoded = evidence.to_string();
    if encoded.len() > 512 * 1024 {
        return Err(conflict("访问审计证据超过512KiB"));
    }
    let observed = now(conn).await?;
    sqlx::query("INSERT INTO data_access_audit(id,tenant_id,actor_id,action,outcome,evidence,created_at) VALUES(?1,?2,?3,'query','succeeded',?4,?5)")
        .bind(&audit.id).bind(&audit.tenant_id).bind(&audit.actor_id).bind(encoded)
        .bind(observed.to_rfc3339()).execute(conn).await.map_err(error)?;
    Ok(())
}

async fn privacy(
    conn: &mut SqliteConnection,
    p: &FinishServiceCallParams,
) -> Result<(), BackendError> {
    let mut actual = Vec::new();
    for kind in ["masking_rule", "user_group"] {
        let rows=sqlx::query("SELECT id,version FROM product_resource WHERE tenant_id=?1 AND kind=?2 AND status='published' ORDER BY id LIMIT 1001")
            .bind(&p.call.tenant_id).bind(kind).fetch_all(&mut *conn).await.map_err(error)?;
        if rows.len() > 1000 {
            return Err(conflict("当前隐私策略超过执行容量"));
        }
        for row in rows {
            actual.push(PrivacyPolicyVersion {
                kind: kind.into(),
                id: row.try_get("id").map_err(error)?,
                version: row.try_get("version").map_err(error)?,
            });
        }
    }
    actual.sort();
    let mut expected = p
        .masking_versions
        .clone()
        .ok_or_else(|| conflict("访问缺少隐私策略版本"))?;
    expected.sort();
    if actual != expected {
        return Err(conflict("调用期间隐私策略已经变化"));
    }
    Ok(())
}

async fn verify_grants(
    conn: &mut SqliteConnection,
    p: &FinishServiceCallParams,
    target: &DataAccessTarget,
) -> Result<(), BackendError> {
    let audit = p
        .access_audit
        .as_ref()
        .ok_or_else(|| conflict("访问证据不存在"))?;
    let evidence = &audit.evidence;
    if evidence.grant_ids.is_empty() || evidence.metadata_ids != [target.metadata_id.clone()] {
        return Err(conflict("原查询授权不属于实际API来源"));
    }
    let at = now(conn).await?;
    let mut seen = BTreeSet::new();
    let mut allowed = BTreeSet::new();
    for id in &evidence.grant_ids {
        if !seen.insert(id) {
            return Err(conflict("原查询授权身份重复"));
        }
        let row=sqlx::query("SELECT g.* FROM data_access_grant g JOIN product_resource r ON r.tenant_id=g.tenant_id AND r.kind='data_access_request' AND r.id=g.request_id
            WHERE g.tenant_id=?1 AND g.grantee_id=?2 AND g.id=?3 AND g.revoked_at IS NULL
            AND julianday(g.expires_at)>julianday(?4)
            AND r.status='approved' AND r.version=g.request_version")
            .bind(&p.call.tenant_id).bind(&p.call.owner_id).bind(id).bind(at.to_rfc3339())
            .fetch_optional(&mut *conn).await.map_err(error)?
            .ok_or_else(|| conflict("原查询授权已撤销或过期"))?;
        let grant = grants::decode(&row)?;
        validation::target(&grant.target, &p.call.tenant_id, &grant.columns)?;
        if grant.target != *target {
            return Err(conflict("原查询授权元数据版本与API不一致"));
        }
        allowed.extend(grant.columns);
    }
    if evidence
        .columns
        .iter()
        .any(|column| !allowed.contains(column))
    {
        return Err(conflict("原查询使用了未授权字段"));
    }
    Ok(())
}
