use super::{PgStandardGovernanceRepo, authority, dependencies, storage::*};
use dts_backend::{
    BackendError,
    repo::{metadata_catalog::*, standard_governance::*},
};
use serde_json::json;
use sqlx::{PgConnection, Row};

pub(super) async fn snapshot(
    conn: &mut PgConnection,
    p: &StandardInspectionParams,
) -> Result<StandardInspectionSnapshot, BackendError> {
    text(&p.model_id)?;
    text(&p.metadata_id)?;
    version(p.expected_model_version)?;
    version(p.expected_binding_version)?;
    version(p.metadata_version)?;
    authority::require(
        conn,
        &p.tenant_id,
        &p.actor_id,
        &[
            "models:read",
            "standards:read",
            "standards:execute",
            "metadata:read",
        ],
    )
    .await?;
    let resource = resource(conn, &p.tenant_id, "data_model", &p.model_id).await?;
    if resource.status != "published" || resource.version != p.expected_model_version {
        return Err(conflict("结构核验需要当前已物理发布模型"));
    }
    let bindings = binding(conn, &p.tenant_id, &p.model_id)
        .await?
        .ok_or_else(|| conflict("模型没有实际标准绑定"))?;
    if bindings.binding_version != p.expected_binding_version
        || bindings.model_properties_hash != standard_hash(&resource.properties)?
    {
        return Err(conflict("模型定义或标准绑定已改变"));
    }
    let revisions = dependencies::for_binding(conn, &p.tenant_id, &bindings).await?;
    let metadata =
        super::storage::resource(conn, &p.tenant_id, "metadata_object", &p.metadata_id).await?;
    if metadata.version != p.metadata_version || metadata.status != "published" {
        return Err(conflict("元数据对象版本已改变或尚未实际采集"));
    }
    let row=sqlx::query("SELECT table_catalog,schema_name,table_name,table_type,columns_snapshot::text AS columns_snapshot,source_fingerprint,last_observed_at FROM metadata_catalog WHERE tenant_id=$1 AND object_id=$2 AND object_version=$3 AND engine='swanlake' AND discovery_status='present' FOR SHARE").bind(&p.tenant_id).bind(&p.metadata_id).bind(p.metadata_version).fetch_optional(&mut *conn).await.map_err(error)?.ok_or_else(||conflict("元数据没有当前实际采集证明"))?;
    let table = MetadataTable {
        table_catalog: row.try_get("table_catalog").map_err(error)?,
        schema_name: row.try_get("schema_name").map_err(error)?,
        table_name: row.try_get("table_name").map_err(error)?,
        table_type: row.try_get("table_type").map_err(error)?,
        columns: decode(
            &row.try_get::<String, _>("columns_snapshot")
                .map_err(error)?,
        )?,
    };
    let binding = MetadataEngineBinding {
        engine: "swanlake".into(),
        tenant_id: p.tenant_id.clone(),
        object_version: p.metadata_version,
        table_catalog: table.table_catalog.clone(),
        schema_name: table.schema_name.clone(),
        table_name: table.table_name.clone(),
        source_fingerprint: row.try_get("source_fingerprint").map_err(error)?,
    };
    if !binding_matches(&binding, &metadata.properties)
        || metadata.properties["columns"] != json!(table.columns)
        || binding.source_fingerprint != table.fingerprint()?
    {
        return Err(conflict("内部采集证明与当前元数据结构不一致"));
    }
    let metadata_observed_at = row.try_get("last_observed_at").map_err(error)?;
    let ids=sqlx::query_scalar::<_,String>("SELECT r.id FROM product_resource r JOIN product_resource_links l ON l.tenant_id=r.tenant_id AND l.source_kind=r.kind AND l.source_id=r.id WHERE r.tenant_id=$1 AND r.kind='model_publication' AND r.status='published' AND l.field='model_id' AND l.target_kind='data_model' AND l.target_id=$2 AND r.properties->'model_version'=to_jsonb($3::bigint) ORDER BY r.created_at DESC,r.id LIMIT 2 FOR SHARE OF r,l").bind(&p.tenant_id).bind(&p.model_id).bind(resource.version-1).fetch_all(&mut *conn).await.map_err(error)?;
    if ids.len() != 1 {
        return Err(conflict("当前模型没有唯一真实成功的物理发布回执"));
    }
    let publication =
        super::storage::resource(conn, &p.tenant_id, "model_publication", &ids[0]).await?;
    let receipt = &publication.properties["receipt"];
    let physical = &receipt["physical_binding"];
    if !super::physical::receipt_valid(&resource.properties, receipt)
        || physical["table_catalog"] != binding.table_catalog
        || physical["schema_name"] != binding.schema_name
        || physical["table_name"] != binding.table_name
        || physical["table_type"] != table.table_type
    {
        return Err(conflict("实际采集的物理表与当前模型发布身份不一致"));
    }
    let standards = &publication.properties["standards"];
    let expected = revisions.iter().map(|v| v.reference()).collect::<Vec<_>>();
    let recorded: Vec<FrozenStandardRef> =
        serde_json::from_value(standards["dependencies"].clone())
            .map_err(|_| conflict("模型发布缺少真实冻结标准依赖"))?;
    if standards["state"] != "validated"
        || standards["binding_version"] != bindings.binding_version
        || standards["binding_hash"] != bindings.definition_hash
        || dependencies::keys(&recorded)? != dependencies::keys(&expected)?
        || recorded.len() != expected.len()
    {
        return Err(conflict("模型发布标准证明与当前有效修订不一致"));
    }
    let mut snapshot = StandardInspectionSnapshot {
        resource,
        bindings,
        revisions,
        metadata,
        binding,
        metadata_observed_at,
        publication,
        snapshot_hash: String::new(),
    };
    let mut value = serde_json::to_value(&snapshot).map_err(|_| invalid("核验证据无法编码"))?;
    value
        .as_object_mut()
        .ok_or_else(|| invalid("核验证据结构无效"))?
        .remove("snapshot_hash");
    if encode(&value)?.len() > 1048576 {
        return Err(invalid("结构核验证据超过1MiB"));
    }
    snapshot.snapshot_hash = standard_hash(&value)?;
    Ok(snapshot)
}
pub(super) async fn record(
    repo: &PgStandardGovernanceRepo,
    p: &RecordStandardInspectionParams,
) -> Result<StandardInspectionResult, BackendError> {
    if uuid::Uuid::parse_str(&p.result.id).is_err() {
        return Err(invalid("结构核验记录标识必须是UUID"));
    }
    let mut tx = repo.begin_write(&p.source.tenant_id).await?;
    let snapshot = snapshot(&mut tx, &p.source).await?;
    let result = &p.result;
    if p.snapshot_hash != snapshot.snapshot_hash
        || result.model_id != snapshot.resource.id
        || result.model_version != snapshot.resource.version
        || result.binding_version != snapshot.bindings.binding_version
        || result.metadata_id != snapshot.metadata.id
        || result.metadata_version != snapshot.metadata.version
        || result.metadata_observed_at != snapshot.metadata_observed_at
        || result.source_fingerprint != snapshot.binding.source_fingerprint
        || result.evaluation_scope != "collected_schema"
        || result.data_values_evaluated
        || result.unit_semantics_evaluated
        || result.issues.len() > 1024
        || !["conformant", "non_conformant"].contains(&result.state.as_str())
        || (result.state == "conformant") != result.issues.is_empty()
    {
        return Err(conflict(
            "核验结果与真实当前结构快照不一致，不能声明数据值或单位换算已通过",
        ));
    }
    let expected = snapshot
        .revisions
        .iter()
        .map(|r| r.reference())
        .collect::<Vec<_>>();
    if dependencies::keys(&result.dependency_versions)? != dependencies::keys(&expected)?
        || result.dependency_versions.len() != expected.len()
    {
        return Err(conflict("核验结果冻结依赖与当前真实修订不一致"));
    }
    for issue in &result.issues {
        if issue.code.is_empty()
            || issue.code.len() > 128
            || issue.message.trim().is_empty()
            || issue.message.len() > 2000
            || issue.message.chars().any(char::is_control)
        {
            return Err(invalid("结构核验差异说明无效"));
        }
    }
    let mut result = result.clone();
    result.created_at = chrono::Utc::now();
    let payload = encode(&result)?;
    if payload.len() > 524288 {
        return Err(invalid("结构核验结果超过512KiB"));
    }
    sqlx::query("INSERT INTO standard_inspection(tenant_id,id,model_id,binding_version,metadata_id,actor_id,snapshot_hash,payload) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(&p.source.tenant_id).bind(&result.id).bind(&result.model_id).bind(result.binding_version).bind(&result.metadata_id).bind(&p.source.actor_id).bind(&p.snapshot_hash).bind(payload).execute(&mut *tx).await.map_err(error)?;
    super::transitions::audit(
        &mut tx,
        &StandardScope {
            tenant_id: p.source.tenant_id.clone(),
            actor_id: p.source.actor_id.clone(),
            kind: "data_model".into(),
            id: result.model_id.clone(),
            expected_version: result.model_version,
        },
        "standards_inspect",
        Some(result.binding_version),
    )
    .await?;
    tx.commit().await.map_err(error)?;
    Ok(result)
}
