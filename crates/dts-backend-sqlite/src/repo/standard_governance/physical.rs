use super::storage::*;
use dts_backend::{
    BackendError, ProductResource, ResourceReference, ResourceWrite, repo::standard_governance::*,
};
use serde_json::Value;
use sqlx::SqliteConnection;
pub(super) fn receipt_valid(model: &Value, receipt: &Value) -> bool {
    let Some(binding) = receipt.get("physical_binding") else {
        return false;
    };
    let Some(catalog) = binding["table_catalog"].as_str().filter(|v| !v.is_empty()) else {
        return false;
    };
    if catalog.len() > 1024 || catalog.chars().any(char::is_control) {
        return false;
    }
    let (Some(schema), Some(table), Some(target)) = (
        binding["schema_name"].as_str(),
        binding["table_name"].as_str(),
        model["table_name"].as_str(),
    ) else {
        return false;
    };
    let pieces = target.split('.').collect::<Vec<_>>();
    let target_matches = match pieces.as_slice() {
        [name] => schema == "main" && table == *name,
        [namespace, name] => schema == *namespace && table == *name,
        _ => false,
    };
    if !receipt["reconciled"].is_boolean()
        || !target_matches
        || binding["table_type"] != "BASE TABLE"
        || receipt["generated_ddl"]
            .as_str()
            .is_none_or(|v| v.trim().is_empty())
    {
        return false;
    }
    let (Some(expected), Some(actual)) = (
        model["columns"].as_array(),
        receipt["physical_columns"].as_array(),
    ) else {
        return false;
    };
    if expected.is_empty() || expected.len() != actual.len() {
        return false;
    }
    expected.iter().zip(actual).all(|(column, actual_column)| {
        let Some(name) = column["name"].as_str() else {
            return false;
        };
        let matching = actual
            .iter()
            .filter(|v| {
                v["column_name"]
                    .as_str()
                    .is_some_and(|v| v.eq_ignore_ascii_case(name))
            })
            .collect::<Vec<_>>();
        let primary_key = model["primary_key"].as_array().is_some_and(|keys| {
            keys.iter().any(|key| {
                key.as_str()
                    .is_some_and(|key| key.eq_ignore_ascii_case(name))
            })
        });
        actual_column["column_name"] == column["name"]
            && matching.len() == 1
            && matching[0]["data_type"]
                .as_str()
                .zip(column["data_type"].as_str())
                .is_some_and(|(physical, declared)| {
                    canonical_type(physical).is_some()
                        && canonical_type(physical) == canonical_type(declared)
                })
            && matching[0]["is_nullable"].as_str()
                == Some(
                    if column["nullable"].as_bool().unwrap_or(true) && !primary_key {
                        "YES"
                    } else {
                        "NO"
                    },
                )
    })
}
fn canonical_type(value: &str) -> Option<String> {
    let normalized = value.trim().to_ascii_uppercase();
    let simple = match normalized.as_str() {
        "BOOL" | "BOOLEAN" => "BOOLEAN",
        "TINYINT" => "TINYINT",
        "SMALLINT" | "INT2" => "SMALLINT",
        "INT" | "INTEGER" | "INT4" => "INTEGER",
        "BIGINT" | "INT8" => "BIGINT",
        "HUGEINT" => "HUGEINT",
        "UTINYINT" => "UTINYINT",
        "USMALLINT" => "USMALLINT",
        "UINTEGER" => "UINTEGER",
        "UBIGINT" => "UBIGINT",
        "UHUGEINT" => "UHUGEINT",
        "REAL" | "FLOAT" => "FLOAT",
        "DOUBLE" | "DOUBLE PRECISION" => "DOUBLE",
        "TEXT" | "STRING" | "VARCHAR" => "VARCHAR",
        "BLOB" | "BYTEA" => "BLOB",
        "DATE" => "DATE",
        "TIME" => "TIME",
        "TIMESTAMP" | "DATETIME" => "TIMESTAMP",
        "TIMESTAMPTZ" | "TIMESTAMP WITH TIME ZONE" => "TIMESTAMPTZ",
        "INTERVAL" => "INTERVAL",
        "UUID" => "UUID",
        "JSON" => "JSON",
        _ => "",
    };
    if !simple.is_empty() {
        return Some(simple.into());
    }
    let parameters = normalized
        .strip_prefix("DECIMAL")
        .or_else(|| normalized.strip_prefix("NUMERIC"))?
        .trim()
        .strip_prefix('(')?
        .strip_suffix(')')?;
    let mut parts = parameters.split(',').map(str::trim);
    let precision = parts.next()?.parse::<u8>().ok()?;
    let scale = parts.next()?.parse::<u8>().ok()?;
    (parts.next().is_none() && (1..=38).contains(&precision) && scale <= precision)
        .then(|| format!("DECIMAL({precision},{scale})"))
}
pub(super) async fn publication(
    conn: &mut SqliteConnection,
    claim: &StandardModelPublicationClaimRecord,
    model: &ProductResource,
    status: &str,
    properties: Value,
) -> Result<ProductResource, BackendError> {
    let now = chrono::Utc::now();
    let id = uuid::Uuid::new_v4().to_string();
    let public_status = match status {
        "published" => "published",
        "publish_failed" => "failed",
        _ => "unknown",
    };
    let after = ProductResource {
        id: id.clone(),
        tenant_id: claim.tenant_id.clone(),
        kind: "model_publication".into(),
        code: id.clone(),
        name: format!("{} 发布记录", model.name),
        description: None,
        parent_id: None,
        status: public_status.into(),
        properties,
        version: 1,
        created_by: claim.actor_id.clone(),
        created_at: now,
        updated_at: now,
        references: vec![ResourceReference {
            field: "model_id".into(),
            target_kind: "data_model".into(),
            target_id: model.id.clone(),
        }],
    };
    let fields = ResourceWrite {
        code: after.code.clone(),
        name: after.name.clone(),
        description: None,
        parent_id: None,
        status: after.status.clone(),
        properties: after.properties.clone(),
        references: after.references.clone(),
    };
    super::super::product_resource::helpers::validate_graph(
        conn,
        &after.tenant_id,
        &after.kind,
        &after.id,
        &fields,
    )
    .await?;
    sqlx::query("INSERT INTO product_resource(id,tenant_id,kind,code,name,status,properties,version,created_by,created_at,updated_at) VALUES(?1,?2,'model_publication',?1,?3,?4,?5,1,?6,?7,?7)").bind(&id).bind(&claim.tenant_id).bind(&after.name).bind(&after.status).bind(encode(&after.properties)?).bind(&claim.actor_id).bind(now.to_rfc3339()).execute(&mut *conn).await.map_err(error)?;
    super::super::product_resource::SqliteProductResourceRepo::write_references(
        conn,
        &after.tenant_id,
        &after.kind,
        &after.id,
        &after.references,
    )
    .await?;
    super::super::product_resource::helpers::audit(
        conn,
        &claim.actor_id,
        "create",
        None,
        Some(&after),
    )
    .await?;
    Ok(after)
}
