// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{BackendError, repo::product_job::ProductJobLease};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetadataScope {
    pub engine: String,
    pub table_catalog: Option<String>,
    pub schema_name: Option<String>,
    pub table_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetadataColumn {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub ordinal_position: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetadataTable {
    pub table_catalog: String,
    pub schema_name: String,
    pub table_name: String,
    pub table_type: String,
    pub columns: Vec<MetadataColumn>,
}

impl MetadataScope {
    pub fn contains(&self, table: &MetadataTable) -> bool {
        self.table_catalog
            .as_ref()
            .is_none_or(|value| value == &table.table_catalog)
            && self
                .schema_name
                .as_ref()
                .is_none_or(|value| value == &table.schema_name)
            && self
                .table_name
                .as_ref()
                .is_none_or(|value| value == &table.table_name)
    }
}

impl MetadataTable {
    pub fn fingerprint(&self) -> Result<String, BackendError> {
        let bytes = serde_json::to_vec(self).map_err(|_| invalid("元数据结构无法编码"))?;
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }

    pub fn identity_code(&self, tenant: &str, engine: &str) -> Result<String, BackendError> {
        let bytes = serde_json::to_vec(&(
            tenant,
            engine,
            &self.table_catalog,
            &self.schema_name,
            &self.table_name,
        ))
        .map_err(|_| invalid("元数据身份无法编码"))?;
        Ok(format!("metadata_{:x}", Sha256::digest(bytes)))
    }

    /// Collector-owned projection. It never copies a request-supplied binding.
    pub fn properties(&self, tenant: &str) -> Result<Value, BackendError> {
        Ok(json!({
            "source_type":"swanlake", "table_catalog":self.table_catalog,
            "schema_name":self.schema_name, "table_name":self.table_name,
            "object_type":if self.table_type == "VIEW" {"view"} else {"table"},
            "columns":self.columns, "source_fingerprint":self.fingerprint()?,
            "discovery_status":"present",
            "engine_binding":{"engine":"swanlake","tenant_id":tenant}
        }))
    }
}

pub struct MetadataApplyParams {
    pub tenant_id: String,
    pub actor_id: String,
    pub collection_id: String,
    pub collection_version: i64,
    pub collection_properties: Value,
    pub scope: MetadataScope,
    pub tables: Vec<MetadataTable>,
    pub job_id: String,
    pub node_id: String,
    pub lease: ProductJobLease,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetadataApplyResult {
    pub collected_tables: u32,
    pub collected_columns: u32,
    pub created_objects: u32,
    pub updated_objects: u32,
    pub unchanged_objects: u32,
    pub missing_objects: u32,
    pub scope: MetadataScope,
    pub reflection_committed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetadataEngineBinding {
    pub engine: String,
    pub tenant_id: String,
    pub object_version: i64,
    pub table_catalog: String,
    pub schema_name: String,
    pub table_name: String,
    pub source_fingerprint: String,
}

/// Reflection and the collecting node's observed success commit together.
/// The live job lease, actor, collection version and full scope are mandatory.
#[async_trait]
pub trait MetadataCatalogRepo: Send + Sync {
    async fn apply_snapshot(
        &self,
        params: &MetadataApplyParams,
    ) -> Result<MetadataApplyResult, BackendError>;
    async fn verified_binding(
        &self,
        tenant_id: &str,
        object_id: &str,
        object_version: i64,
    ) -> Result<Option<MetadataEngineBinding>, BackendError>;
}

pub fn validate_snapshot(params: &MetadataApplyParams) -> Result<(), BackendError> {
    if params.scope.engine != "swanlake"
        || params.collection_version <= 0
        || params.lease.fence <= 0
    {
        return Err(invalid("元数据范围、来源版本或执行租约无效"));
    }
    if params.collection_properties["source_type"] != "swanlake"
        || params.collection_properties.get("engine_binding").is_some()
        || params
            .collection_properties
            .get("datasource_id")
            .is_some_and(|value| !value.is_null() && value.as_str() != Some(""))
    {
        return Err(invalid("采集来源不是实际受控分析引擎"));
    }
    for (key, filter) in [
        ("catalog_filter", &params.scope.table_catalog),
        ("schema_filter", &params.scope.schema_name),
        ("table_filter", &params.scope.table_name),
    ] {
        let actual = params
            .collection_properties
            .get(key)
            .filter(|value| !value.is_null())
            .map(|value| {
                value
                    .as_str()
                    .ok_or_else(|| invalid("采集定义筛选字段无效"))
            })
            .transpose()?
            .filter(|value| !value.trim().is_empty());
        if actual != filter.as_deref() {
            return Err(invalid("采集范围与实际保存定义不一致"));
        }
    }
    for id in [
        &params.tenant_id,
        &params.actor_id,
        &params.collection_id,
        &params.job_id,
        &params.node_id,
        &params.lease.worker_id,
    ] {
        bounded(id, 128)?;
    }
    for filter in [
        &params.scope.table_catalog,
        &params.scope.schema_name,
        &params.scope.table_name,
    ]
    .into_iter()
    .flatten()
    {
        bounded(filter, 256)?;
    }
    if params.tables.len() > 1000 {
        return Err(invalid("单次采集超过 1000 个表，请缩小范围"));
    }
    let mut identities = BTreeSet::new();
    let mut count = 0usize;
    for table in &params.tables {
        for field in [&table.table_catalog, &table.schema_name, &table.table_name] {
            bounded(field, 256)?;
        }
        if !params.scope.contains(table)
            || !["BASE TABLE", "VIEW"].contains(&table.table_type.as_str())
            || !identities.insert((&table.table_catalog, &table.schema_name, &table.table_name))
        {
            return Err(invalid("采集表超出范围、类型无效或重复"));
        }
        let mut names = BTreeSet::new();
        let mut previous = 0;
        for column in &table.columns {
            bounded(&column.name, 256)?;
            bounded(&column.data_type, 512)?;
            if column.ordinal_position <= previous || !names.insert(&column.name) {
                return Err(invalid("字段名称重复或顺序位置无效"));
            }
            previous = column.ordinal_position;
            count += 1;
        }
    }
    if count > 10000
        || serde_json::to_vec(&params.tables)
            .map_err(|_| invalid("元数据无法编码"))?
            .len()
            > 8 * 1024 * 1024
    {
        return Err(invalid("完整元数据快照超过采集容量，未应用部分结果"));
    }
    Ok(())
}

pub fn binding_matches(binding: &MetadataEngineBinding, properties: &Value) -> bool {
    properties["source_type"] == "swanlake"
        && properties["table_catalog"] == binding.table_catalog
        && properties["schema_name"] == binding.schema_name
        && properties["table_name"] == binding.table_name
        && properties["source_fingerprint"] == binding.source_fingerprint
        && properties["discovery_status"] == "present"
        && properties["engine_binding"]["engine"] == binding.engine
        && properties["engine_binding"]["tenant_id"] == binding.tenant_id
}

pub fn bounded(value: &str, max: usize) -> Result<(), BackendError> {
    if value.is_empty() || value.len() > max || value.chars().any(char::is_control) {
        Err(invalid("元数据字段为空、过长或包含控制字符"))
    } else {
        Ok(())
    }
}
fn invalid(message: &str) -> BackendError {
    BackendError::ConstraintViolation(message.into())
}
