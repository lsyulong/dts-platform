// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use dts_backend::BackendError;
use dts_engine::{SyncConfig, SyncMode, TableMapping};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PipelineConfig {
    #[serde(default)]
    pub table_mappings: Vec<TableMapping>,
    #[serde(default, alias = "mode")]
    pub sync_mode: SyncMode,
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter_config: Option<serde_json::Value>,
}

pub fn default_batch_size() -> usize {
    1000
}

impl PipelineConfig {
    pub fn parse(json: &str) -> Result<Self, BackendError> {
        let config: Self = serde_json::from_str(json)
            .map_err(|e| BackendError::ConstraintViolation(format!("任务配置无效: {e}")))?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), BackendError> {
        if self.filter_config.as_ref().is_some_and(|filter| {
            !filter.is_null() && !filter.as_object().is_some_and(|object| object.is_empty())
        }) {
            return Err(BackendError::ConstraintViolation(
                "当前不支持行过滤配置；仅支持明确的表映射".into(),
            ));
        }
        self.as_sync_config()
            .validate()
            .map_err(|e| BackendError::ConstraintViolation(e.to_string()))
    }

    pub fn as_sync_config(&self) -> SyncConfig {
        SyncConfig {
            table_mappings: self.table_mappings.clone(),
            mode: self.sync_mode.clone(),
            batch_size: self.batch_size,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_or_unsupported_pipeline_configs_are_not_defaulted() {
        for config in [
            "not json",
            "{}",
            r#"{"table_mappings":{"users":"backup"}}"#,
            r#"{"table_mappings":[{"source_table":"users","target_table":"backup"}],"sync_mode":"incremental"}"#,
            r#"{"table_mappings":[{"source_table":"users","target_table":"backup"}],"sync_mode":"typo"}"#,
            r#"{"table_mappings":[{"source_table":"users","target_table":"backup"}],"batch_size":0}"#,
            r#"{"table_mappings":[{"source_table":"users","target_table":"backup"}],"filter_config":{"status":"active"}}"#,
            r#"{"table_mappings":[{"source_table":"users","target_table":"backup"}],"batch_szie":100}"#,
        ] {
            assert!(
                PipelineConfig::parse(config).is_err(),
                "config should be rejected: {config}"
            );
        }
    }

    #[test]
    fn mappings_and_defaults_survive_config_roundtrip() -> anyhow::Result<()> {
        let json = r#"{"table_mappings":[{"source_table":"orders","target_table":"fact_orders","target_schema":"mart"}]}"#;
        let config = PipelineConfig::parse(json)?;
        assert_eq!(config.batch_size, 1000);
        assert_eq!(config.sync_mode, SyncMode::Full);
        let persisted = serde_json::to_string(&config)?;
        let parsed = PipelineConfig::parse(&persisted)?;
        assert_eq!(
            parsed.table_mappings[0].target_schema.as_deref(),
            Some("mart")
        );
        Ok(())
    }
}
