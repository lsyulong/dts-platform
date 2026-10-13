use super::{PgProductResourceRepo, helpers};
use async_trait::async_trait;
use dts_backend::{
    BackendError, ResourceReference, ResourceWrite,
    repo::{metadata_catalog::*, metadata_lineage::*},
};
use serde_json::{Value, json};
use sqlx::{PgConnection, Row};
use std::collections::{HashMap, HashSet, VecDeque};

pub(super) fn validate_actor(
    kind: &str,
    fields: &ResourceWrite,
    actor: &str,
) -> Result<(), BackendError> {
    if kind == "metadata_lineage" && fields.properties["evidence_actor_id"].as_str() != Some(actor)
    {
        return Err(invalid("血缘证据操作人必须是当前实际写入用户"));
    }
    Ok(())
}

pub(super) async fn validate(
    conn: &mut PgConnection,
    tenant: &str,
    id: &str,
    fields: &ResourceWrite,
) -> Result<(), BackendError> {
    let edge = decode_edge(id, 1, fields)?;
    if edge.source_object_id == edge.target_object_id {
        return Err(invalid("血缘对象不能引用自身"));
    }
    for (field, kind, target) in [
        (
            "source_object_id",
            "metadata_object",
            &edge.source_object_id,
        ),
        (
            "target_object_id",
            "metadata_object",
            &edge.target_object_id,
        ),
        ("evidence_actor_id", "user", &edge.evidence_actor_id),
    ] {
        if !fields.references.contains(&ResourceReference {
            field: field.into(),
            target_kind: kind.into(),
            target_id: target.clone(),
        }) {
            return Err(invalid("血缘证据缺少真实端点或操作人引用"));
        }
    }
    let actor: Option<String> = sqlx::query_scalar(
        "SELECT u.id FROM \"user\" u JOIN tenant t ON t.id=u.tenant_id
        WHERE t.id=$1 AND u.id=$2 AND t.status='active' AND u.status='active' FOR SHARE OF u",
    )
    .bind(tenant)
    .bind(&edge.evidence_actor_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(helpers::map_error)?;
    if actor.is_none() {
        return Err(invalid("血缘证据操作人或租户已失效"));
    }
    for (endpoint, version) in [
        (&edge.source_object_id, edge.source_object_version),
        (&edge.target_object_id, edge.target_object_version),
    ] {
        if node(conn, tenant, endpoint)
            .await?
            .is_none_or(|node| node.version != version)
        {
            return Err(invalid("血缘端点没有当前真实元数据证明或版本已变化"));
        }
    }
    let rows=sqlx::query("SELECT s.target_id AS source,t.target_id AS target FROM product_resource_links s JOIN product_resource_links t
        ON t.tenant_id=s.tenant_id AND t.source_kind=s.source_kind AND t.source_id=s.source_id JOIN product_resource r
        ON r.tenant_id=s.tenant_id AND r.kind=s.source_kind AND r.id=s.source_id WHERE s.tenant_id=$1 AND s.source_kind='metadata_lineage'
        AND s.field='source_object_id' AND t.field='target_object_id' AND s.target_kind='metadata_object' AND t.target_kind='metadata_object'
        AND r.status IN ('draft','published') AND s.source_id<>$2 LIMIT 2001")
        .bind(tenant).bind(id).fetch_all(&mut *conn).await.map_err(helpers::map_error)?;
    if rows.len() >= MAX_LINEAGE_EDGES as usize {
        return Err(invalid("当前血缘超过2000条，请先清理或缩小图范围"));
    }
    let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();
    for row in rows {
        let source: String = row.try_get("source").map_err(helpers::map_error)?;
        let target: String = row.try_get("target").map_err(helpers::map_error)?;
        if source == edge.source_object_id && target == edge.target_object_id {
            return Err(BackendError::Conflict(
                "两个对象之间已经保存此血缘关系".into(),
            ));
        }
        adjacency.entry(source).or_default().push(target);
    }
    let mut queue = VecDeque::from([edge.target_object_id.clone()]);
    let mut visited = HashSet::new();
    while let Some(current) = queue.pop_front() {
        if current == edge.source_object_id {
            return Err(invalid("当前血缘关系会形成依赖环"));
        }
        if visited.insert(current.clone()) {
            for next in adjacency.get(&current).into_iter().flatten() {
                queue.push_back(next.clone());
            }
        }
    }
    Ok(())
}

pub(super) async fn node(
    conn: &mut PgConnection,
    tenant: &str,
    id: &str,
) -> Result<Option<LineageNode>, BackendError> {
    let row=sqlx::query("SELECT r.id,r.code,r.name,r.version,r.status,r.properties::text AS properties,c.engine,c.table_catalog,c.schema_name,c.table_name,c.table_type,
        c.source_fingerprint,c.columns_snapshot::text AS encoded_columns FROM product_resource r JOIN metadata_catalog c
        ON c.tenant_id=r.tenant_id AND c.object_id=r.id WHERE r.tenant_id=$1 AND r.kind='metadata_object' AND r.id=$2
        AND r.status='published' AND c.discovery_status='present' AND c.object_version=r.version")
        .bind(tenant).bind(id).fetch_optional(&mut *conn).await.map_err(helpers::map_error)?;
    let Some(row) = row else { return Ok(None) };
    let table = MetadataTable {
        table_catalog: row.try_get("table_catalog").map_err(helpers::map_error)?,
        schema_name: row.try_get("schema_name").map_err(helpers::map_error)?,
        table_name: row.try_get("table_name").map_err(helpers::map_error)?,
        table_type: row.try_get("table_type").map_err(helpers::map_error)?,
        columns: serde_json::from_str(
            &row.try_get::<String, _>("encoded_columns")
                .map_err(helpers::map_error)?,
        )
        .map_err(|_| invalid("真实元数据字段无效"))?,
    };
    let binding = MetadataEngineBinding {
        engine: row.try_get("engine").map_err(helpers::map_error)?,
        tenant_id: tenant.into(),
        object_version: row.try_get("version").map_err(helpers::map_error)?,
        table_catalog: table.table_catalog.clone(),
        schema_name: table.schema_name.clone(),
        table_name: table.table_name.clone(),
        source_fingerprint: row
            .try_get("source_fingerprint")
            .map_err(helpers::map_error)?,
    };
    let properties: Value = serde_json::from_str(
        &row.try_get::<String, _>("properties")
            .map_err(helpers::map_error)?,
    )
    .map_err(|_| invalid("元数据属性无效"))?;
    if !binding_matches(&binding, &properties)
        || properties["columns"] != json!(table.columns)
        || binding.source_fingerprint != table.fingerprint()?
    {
        return Ok(None);
    }
    Ok(Some(LineageNode {
        id: row.try_get("id").map_err(helpers::map_error)?,
        code: row.try_get("code").map_err(helpers::map_error)?,
        name: row.try_get("name").map_err(helpers::map_error)?,
        version: binding.object_version,
        status: "published".into(),
        table_catalog: table.table_catalog,
        schema_name: table.schema_name,
        table_name: table.table_name,
        source_fingerprint: binding.source_fingerprint,
    }))
}

#[async_trait]
impl MetadataLineageRepo for PgProductResourceRepo {
    async fn snapshot(
        &self,
        params: &LineageSnapshotParams,
    ) -> Result<LineageSnapshot, BackendError> {
        let mut tx = self.pool.begin().await.map_err(helpers::map_error)?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await
            .map_err(helpers::map_error)?;
        let authority:Option<String>=sqlx::query_scalar("SELECT u.id FROM \"user\" u JOIN tenant t ON t.id=u.tenant_id
            WHERE t.id=$1 AND u.id=$2 AND t.status='active' AND u.status='active' AND EXISTS(SELECT 1 FROM user_role ur
                JOIN role r ON r.id=ur.role_id JOIN role_permission rp ON rp.role_id=r.id WHERE ur.user_id=u.id AND r.tenant_id=t.id AND rp.permission='metadata:read')")
            .bind(&params.tenant_id).bind(&params.actor_id).fetch_optional(&mut *tx).await.map_err(helpers::map_error)?;
        if authority.is_none() {
            return Err(invalid("当前用户、租户或 metadata:read 权限已失效"));
        }
        let total:i64=sqlx::query_scalar("SELECT COUNT(*) FROM product_resource WHERE tenant_id=$1 AND kind='metadata_lineage' AND status IN ('draft','published')")
            .bind(&params.tenant_id).fetch_one(&mut *tx).await.map_err(helpers::map_error)?;
        let ids:Vec<String>=sqlx::query_scalar("SELECT id FROM product_resource WHERE tenant_id=$1 AND kind='metadata_lineage' AND status IN ('draft','published') ORDER BY id LIMIT $2")
            .bind(&params.tenant_id).bind(i64::from(params.max_edges.clamp(1,MAX_LINEAGE_EDGES))).fetch_all(&mut *tx).await.map_err(helpers::map_error)?;
        let scanned_edges = ids.len();
        let mut edges = Vec::new();
        let mut invalid_edges = Vec::new();
        let mut nodes: HashMap<String, Option<LineageNode>> = HashMap::new();
        let mut bytes = 0usize;
        if let Some(root) = &params.root_object_id {
            nodes.insert(root.clone(), node(&mut tx, &params.tenant_id, root).await?);
        }
        for id in ids {
            let Some(resource) =
                Self::read_resource(&mut tx, &params.tenant_id, "metadata_lineage", &id).await?
            else {
                continue;
            };
            let fields = ResourceWrite {
                code: resource.code,
                name: resource.name,
                description: resource.description,
                parent_id: resource.parent_id,
                status: resource.status,
                properties: resource.properties,
                references: resource.references,
            };
            let mut edge = match decode_edge(&id, resource.version, &fields) {
                Ok(edge) => edge,
                Err(_) => {
                    invalid_edges.push(InvalidLineageEdge {
                        id,
                        version: resource.version,
                        reason: "unverified_legacy_or_invalid_provenance".into(),
                    });
                    continue;
                },
            };
            bytes += edge.evidence.len();
            if bytes > 8 * 1024 * 1024 {
                return Err(invalid("血缘证据总量超过8MiB，请缩小扫描范围"));
            }
            for (endpoint, version) in [
                (&edge.source_object_id, edge.source_object_version),
                (&edge.target_object_id, edge.target_object_version),
            ] {
                if !nodes.contains_key(endpoint) {
                    nodes.insert(
                        endpoint.clone(),
                        node(&mut tx, &params.tenant_id, endpoint).await?,
                    );
                }
                if let Some(current) = nodes[endpoint].as_ref() {
                    if current.version != version {
                        edge.stale = true;
                        edge.invalid_reason = Some("endpoint_version_changed".into());
                    }
                } else {
                    edge.stale = true;
                    edge.invalid_reason = Some("endpoint_not_current_or_unverified".into());
                }
            }
            edges.push(edge);
        }
        tx.commit().await.map_err(helpers::map_error)?;
        let scan_truncated = total > scanned_edges as i64;
        let mut nodes: Vec<LineageNode> = nodes.into_values().flatten().collect();
        nodes.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(LineageSnapshot {
            nodes,
            edges,
            invalid_edges,
            total_edges: total,
            scanned_edges,
            scan_truncated,
        })
    }
}
fn invalid(message: &str) -> BackendError {
    BackendError::ConstraintViolation(message.into())
}
