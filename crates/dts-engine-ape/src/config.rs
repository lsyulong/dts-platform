// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use dts_engine::{ConnectionConfig, EngineError, TaskSubmission};

pub(crate) struct ApeDtsIniConfig(String);

impl ApeDtsIniConfig {
    pub fn from_submission(task: &TaskSubmission, run_id: &str) -> Result<Self, EngineError> {
        task.config.validate()?;
        let source_type = db_type(&task.source)?;
        let target_type = db_type(&task.target)?;
        let source_url = build_url(&task.source)?;
        let target_url = build_url(&task.target)?;
        let mut filters = Vec::new();
        let mut routes = Vec::new();
        let mut effective_sources = std::collections::HashSet::new();
        let mut effective_targets = std::collections::HashSet::new();
        for mapping in &task.config.table_mappings {
            let source_schema = schema(&task.source, mapping.source_schema.as_deref())?;
            let target_schema = schema(&task.target, mapping.target_schema.as_deref())?;
            let source = format!("{source_schema}.{}", mapping.source_table);
            let target = format!("{target_schema}.{}", mapping.target_table);
            if !effective_sources.insert(source.clone())
                || !effective_targets.insert(target.clone())
            {
                return Err(EngineError::InvalidConfig(
                    "源表和目标表的有效映射不可重复".into(),
                ));
            }
            filters.push(source.clone());
            routes.push(format!("{source}:{target}"));
        }
        if run_id.is_empty() || run_id.chars().any(|c| c.is_control()) {
            return Err(EngineError::InvalidConfig("PipelineRun ID 无效".into()));
        }
        // Protocol follows ape-dts dt-common IniLoader / TaskConfig, not TOML.
        Ok(Self(format!(
            "[global]\ntask_id={run_id}\n\n[extractor]\ndb_type={source_type}\nextract_type=snapshot\nurl={source_url}\nbatch_size={batch}\n\n[sinker]\ndb_type={target_type}\nsink_type=write\nurl={target_url}\nbatch_size={batch}\nreplace=false\ndisable_foreign_key_checks=false\n\n[filter]\ndo_tbs={filter}\ndo_events=insert\n\n[router]\ntb_map={route}\n\n[parallelizer]\nparallel_type=snapshot\nparallel_size=1\n\n[pipeline]\nbuffer_size={batch}\nhttp_host=127.0.0.1\nhttp_port=0\n\n[runtime]\nlog_level=info\n",
            batch = task.config.batch_size,
            filter = filters.join(","),
            route = routes.join(","),
        )))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn with_runtime_paths(
        mut self,
        logger: &std::path::Path,
        log_dir: &std::path::Path,
    ) -> Result<Self, EngineError> {
        let logger = logger.to_string_lossy();
        let log_dir = log_dir.to_string_lossy();
        if logger.contains(['\r', '\n']) || log_dir.contains(['\r', '\n']) {
            return Err(EngineError::InvalidConfig(
                "日志配置路径不可包含换行".into(),
            ));
        }
        // The runtime section is the last section emitted by this serializer.
        self.0
            .push_str(&format!("log4rs_file={logger}\nlog_dir={log_dir}\n"));
        Ok(self)
    }
}

fn db_type(conn: &ConnectionConfig) -> Result<&'static str, EngineError> {
    match conn.db_type.to_lowercase().as_str() {
        "mysql" => Ok("mysql"),
        "postgresql" | "postgres" | "pg" => Ok("pg"),
        _ => Err(EngineError::InvalidConfig(
            "当前全量适配器支持 MySQL 和 PostgreSQL；其他连接器待实现".into(),
        )),
    }
}

fn schema<'a>(
    conn: &'a ConnectionConfig,
    explicit: Option<&'a str>,
) -> Result<&'a str, EngineError> {
    let schema = explicit.unwrap_or(if db_type(conn)? == "mysql" {
        &conn.database
    } else {
        "public"
    });
    if schema.is_empty()
        || schema
            .chars()
            .any(|c| c.is_control() || matches!(c, '.' | ',' | ':' | '*' | '`' | '[' | ']' | '='))
    {
        return Err(EngineError::InvalidConfig(
            "数据源默认 schema 不符合映射语法".into(),
        ));
    }
    Ok(schema)
}

fn build_url(conn: &ConnectionConfig) -> Result<String, EngineError> {
    let scheme = if db_type(conn)? == "mysql" {
        "mysql"
    } else {
        "postgresql"
    };
    if conn.host.is_empty()
        || conn.database.is_empty()
        || conn.username.is_empty()
        || conn.port == 0
    {
        return Err(EngineError::InvalidConfig(
            "host、port、username、database 为必填连接配置".into(),
        ));
    }
    let mut url = url::Url::parse(&format!("{scheme}://localhost"))
        .map_err(|e| EngineError::InvalidConfig(e.to_string()))?;
    url.set_host(Some(&conn.host))
        .map_err(|_| EngineError::InvalidConfig("host 无效".into()))?;
    url.set_port(Some(conn.port))
        .map_err(|_| EngineError::InvalidConfig("port 无效".into()))?;
    let username =
        percent_encoding::utf8_percent_encode(&conn.username, percent_encoding::NON_ALPHANUMERIC)
            .to_string();
    let password =
        percent_encoding::utf8_percent_encode(&conn.password, percent_encoding::NON_ALPHANUMERIC)
            .to_string();
    url.set_username(&username)
        .map_err(|_| EngineError::InvalidConfig("username 无效".into()))?;
    url.set_password(Some(&password))
        .map_err(|_| EngineError::InvalidConfig("password 无效".into()))?;
    url.path_segments_mut()
        .map_err(|_| EngineError::InvalidConfig("database 无效".into()))?
        .clear()
        .push(&conn.database);
    Ok(url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use dts_engine::{SyncConfig, SyncMode, TableMapping};

    pub(crate) fn submission() -> TaskSubmission {
        TaskSubmission {
            run_id: "run-1".into(),
            task_id: "pipeline-1".into(),
            source: ConnectionConfig {
                db_type: "mysql".into(),
                host: "localhost".into(),
                port: 3306,
                username: "user".into(),
                password: "p@ss:/?#%".into(),
                database: "sales".into(),
            },
            target: ConnectionConfig {
                db_type: "postgresql".into(),
                host: "localhost".into(),
                port: 5432,
                username: "user".into(),
                password: "pwd".into(),
                database: "warehouse".into(),
            },
            config: SyncConfig {
                mode: SyncMode::Full,
                batch_size: 500,
                table_mappings: vec![TableMapping {
                    source_table: "orders".into(),
                    target_table: "fact_orders".into(),
                    source_schema: None,
                    target_schema: Some("mart".into()),
                }],
            },
        }
    }

    #[test]
    fn config_uses_ape_ini_protocol_and_routes_target_table() {
        let config = ApeDtsIniConfig::from_submission(&submission(), "run-1").unwrap();
        let encoded = config.as_str();
        assert!(
            encoded.contains("extract_type=snapshot\n"),
            "ape expects unquoted INI values"
        );
        assert!(
            encoded.contains("db_type=pg\n"),
            "ape PostgreSQL enum is pg"
        );
        assert!(
            encoded.contains("do_tbs=sales.orders\n"),
            "filter selects mapped source tables"
        );
        assert!(
            encoded.contains("tb_map=sales.orders:mart.fact_orders\n"),
            "target mapping must survive conversion"
        );
        assert!(encoded.contains("parallel_type=snapshot\n"));
        assert!(encoded.contains("replace=false\n"));
        assert!(encoded.contains("disable_foreign_key_checks=false\n"));
        let url = encoded
            .lines()
            .find_map(|l| l.strip_prefix("url="))
            .unwrap();
        let parsed = url::Url::parse(url).unwrap();
        assert_eq!(parsed.password(), Some("p%40ss%3A%2F%3F%23%25"));
        assert_eq!(parsed.host_str(), Some("localhost"));
    }

    #[test]
    fn unsupported_mode_and_source_do_not_silently_become_snapshot() {
        let mut task = submission();
        task.config.mode = SyncMode::Incremental;
        assert!(ApeDtsIniConfig::from_submission(&task, "run-1").is_err());
        task.config.mode = SyncMode::Full;
        task.target.db_type = "duckdb".into();
        assert!(ApeDtsIniConfig::from_submission(&task, "run-1").is_err());
    }
}
