use chrono::{DateTime, Utc};
use dts_backend::{
    BackendError, ProductResource,
    repo::assets::{validation::*, *},
};
use serde_json::{Value, json};
use sqlx::{Row, SqliteConnection};

use super::super::{helpers::map_error, lineage};

pub(super) struct Source {
    pub resource: ProductResource,
    pub proof: Value,
}

pub(super) async fn source(
    conn: &mut SqliteConnection,
    asset: &ProductResource,
) -> Result<Option<Source>, BackendError> {
    let kind = text(&asset.properties, "object_kind")?;
    let id = text(&asset.properties, "object_id")?;
    if !ASSET_SOURCE_KINDS.contains(&kind) {
        return Ok(None);
    }
    let Some(source) =
        super::super::SqliteProductResourceRepo::read_resource(conn, &asset.tenant_id, kind, id)
            .await?
    else {
        return Ok(None);
    };
    if !asset.references.contains(&reference("object_id", kind, id)) {
        return Ok(None);
    }
    if !matches!(source.status.as_str(), "draft" | "published") {
        return Ok(None);
    }
    let proof = match kind {
        "metadata_object" => {
            let Some(node) = lineage::node(conn, &asset.tenant_id, id).await? else {
                return Ok(None);
            };
            json!({"type":"metadata_catalog","engine":"swanlake","source_fingerprint":node.source_fingerprint,
                "table_catalog":node.table_catalog,"schema_name":node.schema_name,"table_name":node.table_name})
        },
        "data_model" => {
            if source.status != "published" {
                return Ok(Some(Source {
                    resource: source,
                    proof: json!({"type":"stored_definition","physical_data_verified":false}),
                }));
            }
            let rows = sqlx::query("SELECT p.id,p.properties FROM product_resource p JOIN product_resource_links l
                ON l.tenant_id=p.tenant_id AND l.source_kind=p.kind AND l.source_id=p.id
                WHERE p.tenant_id=?1 AND p.kind='model_publication' AND p.status='published'
                AND l.field='model_id' AND l.target_kind='data_model' AND l.target_id=?2
                AND json_extract(p.properties,'$.model_version')=?3 ORDER BY p.updated_at DESC,p.id LIMIT 100")
                .bind(&asset.tenant_id).bind(id).bind(source.version.checked_sub(1)).fetch_all(&mut *conn).await.map_err(map_error)?;
            let mut receipt = None;
            for row in rows {
                let properties: Value = serde_json::from_str(
                    &row.try_get::<String, _>("properties").map_err(map_error)?,
                )
                .map_err(|_| invalid("模型发布证明无效"))?;
                if properties["model_version"].as_i64() == source.version.checked_sub(1)
                    && properties["receipt"]["physical_columns"]
                        .as_array()
                        .is_some_and(|v| !v.is_empty())
                {
                    receipt = Some(
                        json!({"type":"model_publication","publication_id":row.try_get::<String,_>("id").map_err(map_error)?,
                        "receipt_hash":hash(&properties["receipt"])?}),
                    );
                    break;
                }
            }
            let Some(receipt) = receipt else {
                return Ok(None);
            };
            receipt
        },
        _ => json!({"type":"stored_definition","physical_data_verified":false}),
    };
    Ok(Some(Source {
        resource: source,
        proof,
    }))
}

pub(super) async fn publication(
    conn: &mut SqliteConnection,
    tenant: &str,
    id: &str,
) -> Result<Option<AssetPublication>, BackendError> {
    let row = sqlx::query("SELECT asset_version,source_kind,source_id,source_version,definition_hash,proof,published_by,published_at
        FROM asset_publication WHERE tenant_id=?1 AND asset_id=?2 ORDER BY asset_version DESC LIMIT 1")
        .bind(tenant).bind(id).fetch_optional(conn).await.map_err(map_error)?;
    let Some(row) = row else { return Ok(None) };
    Ok(Some(AssetPublication {
        asset_version: row.try_get("asset_version").map_err(map_error)?,
        source_kind: row.try_get("source_kind").map_err(map_error)?,
        source_id: row.try_get("source_id").map_err(map_error)?,
        source_version: row.try_get("source_version").map_err(map_error)?,
        definition_hash: row.try_get("definition_hash").map_err(map_error)?,
        proof: serde_json::from_str(&row.try_get::<String, _>("proof").map_err(map_error)?)
            .map_err(|_| invalid("资产发布证明无效"))?,
        published_by: row.try_get("published_by").map_err(map_error)?,
        published_at: DateTime::parse_from_rfc3339(
            &row.try_get::<String, _>("published_at")
                .map_err(map_error)?,
        )
        .map_err(|_| invalid("资产发布时间无效"))?
        .with_timezone(&Utc),
        publication_scope: "catalog_definition".into(),
        physical_published: false,
    }))
}

pub(super) async fn view(
    conn: &mut SqliteConnection,
    resource: ProductResource,
) -> Result<AssetView, BackendError> {
    let publication = publication(conn, &resource.tenant_id, &resource.id).await?;
    let current = match source(conn, &resource).await {
        Ok(value) => value,
        Err(BackendError::ConstraintViolation(_)) => None,
        Err(e) => return Err(e),
    };
    let mut invalid_reason = current
        .is_none()
        .then(|| "source_missing_or_unverified".to_owned());
    let mut stale = false;
    let current_hash = current
        .as_ref()
        .map(|s| hash(&s.resource.properties))
        .transpose()?;
    if let Some(publication) = &publication {
        stale = current.as_ref().is_none_or(|s| {
            publication.source_kind != s.resource.kind
                || publication.source_id != s.resource.id
                || publication.source_version != s.resource.version
                || current_hash.as_ref() != Some(&publication.definition_hash)
                || publication.proof != s.proof
        });
        if stale {
            invalid_reason = Some("published_source_changed_or_unverified".into());
        }
        if resource.status == "published" && publication.asset_version != resource.version {
            stale = true;
            invalid_reason = Some("publication_version_mismatch".into());
        }
    } else if resource.status == "published" {
        stale = true;
        invalid_reason = Some("missing_publication_receipt".into());
    }
    let source_valid = current.is_some() && !stale;
    Ok(AssetView {
        resource,
        publication,
        source_valid,
        stale,
        invalid_reason,
        current_source_version: current.map(|s| s.resource.version),
    })
}

pub(super) async fn publish(
    conn: &mut SqliteConnection,
    asset: &ProductResource,
    source: &Source,
    actor: &str,
) -> Result<(), BackendError> {
    sqlx::query("INSERT INTO asset_publication(tenant_id,asset_id,asset_version,source_kind,source_id,source_version,definition_hash,source_snapshot,proof,published_by,published_at)
        VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)")
        .bind(&asset.tenant_id).bind(&asset.id).bind(asset.version).bind(&source.resource.kind).bind(&source.resource.id).bind(source.resource.version)
        .bind(hash(&source.resource.properties)?).bind(source.resource.properties.to_string()).bind(source.proof.to_string()).bind(actor).bind(Utc::now().to_rfc3339())
        .execute(conn).await.map_err(map_error)?;
    Ok(())
}
