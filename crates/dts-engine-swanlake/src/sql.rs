// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use dts_engine::{ConnectionConfig, EngineError, TableMapping};

const SOURCE_ALIAS: &str = "dts_source";

pub(crate) fn endpoint(connection: &ConnectionConfig) -> Result<String, EngineError> {
    if connection.host.trim().is_empty() || connection.port == 0 {
        return Err(EngineError::InvalidConfig(
            "DuckDB/SwanLake 目标需要 host 和 Flight SQL port".into(),
        ));
    }
    if connection.host.contains("//") {
        let mut url = url::Url::parse(&connection.host)
            .map_err(|_| EngineError::InvalidConfig("SwanLake host 无效".into()))?;
        url.set_port(Some(connection.port))
            .map_err(|_| EngineError::InvalidConfig("SwanLake port 无效".into()))?;
        return Ok(url.to_string().trim_end_matches('/').to_string());
    }
    Ok(format!("http://{}:{}", connection.host, connection.port))
}

pub(crate) fn attach_source(source: &ConnectionConfig) -> Result<Option<String>, EngineError> {
    let kind = normalized(&source.db_type);
    if matches!(kind.as_str(), "duckdb" | "swanlake") {
        return Ok(None);
    }
    let scheme = match kind.as_str() {
        "mysql" => "mysql",
        "pg" | "postgres" | "postgresql" => "postgresql",
        _ => {
            return Err(EngineError::InvalidConfig(format!(
                "SwanLake 引擎不支持 {} 源端",
                source.db_type
            )));
        },
    };
    if source.host.trim().is_empty()
        || source.port == 0
        || source.username.trim().is_empty()
        || source.database.trim().is_empty()
    {
        return Err(EngineError::InvalidConfig(
            "源端 host、port、username、database 为必填连接配置".into(),
        ));
    }
    let mut url = url::Url::parse(&format!("{scheme}://localhost"))
        .map_err(|_| EngineError::InvalidConfig("源端连接配置无效".into()))?;
    url.set_host(Some(&source.host))
        .map_err(|_| EngineError::InvalidConfig("源端 host 无效".into()))?;
    url.set_port(Some(source.port))
        .map_err(|_| EngineError::InvalidConfig("源端 port 无效".into()))?;
    url.set_username(&source.username)
        .map_err(|_| EngineError::InvalidConfig("源端 username 无效".into()))?;
    url.set_password(Some(&source.password))
        .map_err(|_| EngineError::InvalidConfig("源端 password 无效".into()))?;
    url.set_path(&source.database);
    Ok(Some(format!(
        "ATTACH '{}' AS {} (TYPE {})",
        sql_text(url.as_str()),
        quote_identifier(SOURCE_ALIAS),
        kind_for_attach(&kind)?
    )))
}

pub(crate) fn source_table(
    source: &ConnectionConfig,
    mapping: &TableMapping,
) -> Result<String, EngineError> {
    let kind = normalized(&source.db_type);
    let catalog = if matches!(kind.as_str(), "duckdb" | "swanlake") {
        required_identifier(&source.database, "DuckDB 源 catalog")?
    } else {
        SOURCE_ALIAS
    };
    let default_schema = match kind.as_str() {
        "mysql" => source.database.as_str(),
        "pg" | "postgres" | "postgresql" => "public",
        _ => "main",
    };
    let schema = mapping.source_schema.as_deref().unwrap_or(default_schema);
    qualified(catalog, schema, &mapping.source_table)
}

pub(crate) fn target_table(
    target: &ConnectionConfig,
    mapping: &TableMapping,
) -> Result<String, EngineError> {
    let catalog = required_identifier(&target.database, "DuckDB 目标 catalog")?;
    let schema = mapping.target_schema.as_deref().unwrap_or("main");
    qualified(catalog, schema, &mapping.target_table)
}

pub(crate) fn target_schema(
    target: &ConnectionConfig,
    mapping: &TableMapping,
) -> Result<String, EngineError> {
    let catalog = required_identifier(&target.database, "DuckDB 目标 catalog")?;
    let schema = mapping.target_schema.as_deref().unwrap_or("main");
    Ok(format!(
        "{}.{}",
        quote_identifier(catalog),
        quote_identifier(required_identifier(schema, "目标 schema")?)
    ))
}

pub(crate) fn quote_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn qualified(catalog: &str, schema: &str, table: &str) -> Result<String, EngineError> {
    Ok(format!(
        "{}.{}.{}",
        quote_identifier(required_identifier(catalog, "catalog")?),
        quote_identifier(required_identifier(schema, "schema")?),
        quote_identifier(required_identifier(table, "table")?)
    ))
}

fn required_identifier<'a>(value: &'a str, label: &str) -> Result<&'a str, EngineError> {
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        Err(EngineError::InvalidConfig(format!("{label} 无效")))
    } else {
        Ok(value)
    }
}

fn normalized(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn kind_for_attach(kind: &str) -> Result<&'static str, EngineError> {
    match kind {
        "mysql" => Ok("mysql"),
        "pg" | "postgres" | "postgresql" => Ok("postgres"),
        _ => Err(EngineError::InvalidConfig("不支持的 ATTACH 类型".into())),
    }
}

fn sql_text(value: &str) -> String {
    value.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attach_uri_encodes_credentials_and_sql_quotes() {
        let source = ConnectionConfig {
            db_type: "mysql".into(),
            host: "db.internal".into(),
            port: 3306,
            username: "user@tenant".into(),
            password: "p'a:ss".into(),
            database: "sales".into(),
        };
        let sql = attach_source(&source).unwrap().unwrap();
        assert!(sql.starts_with("ATTACH 'mysql://"));
        assert!(!sql.contains("user@tenant:p'a:ss"));
        assert!(sql.contains("TYPE mysql"));
    }

    #[test]
    fn postgres_source_defaults_to_public_schema() {
        let source = ConnectionConfig {
            db_type: "pg".into(),
            host: "db.internal".into(),
            port: 5432,
            username: "reader".into(),
            password: "secret".into(),
            database: "sales".into(),
        };
        let mapping = TableMapping {
            source_table: "orders".into(),
            target_table: "orders".into(),
            source_schema: None,
            target_schema: None,
        };
        assert_eq!(
            source_table(&source, &mapping).unwrap(),
            "\"dts_source\".\"public\".\"orders\""
        );
    }

    #[test]
    fn endpoint_uses_flight_sql_port() {
        let connection = ConnectionConfig {
            db_type: "duckdb".into(),
            host: "swanlake.internal".into(),
            port: 4214,
            username: String::new(),
            password: String::new(),
            database: "warehouse".into(),
        };
        assert_eq!(
            endpoint(&connection).unwrap(),
            "http://swanlake.internal:4214"
        );
    }
}
