// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

//! SwanLake adapter for full loads into DuckDB/DuckLake.
//!
//! The adapter keeps DuckDB execution in SwanLake. It attaches a MySQL or
//! PostgreSQL source inside one Flight SQL session, creates the target table
//! when needed, replaces its rows, and copies data in bounded batches.

mod client;
mod service;
mod sql;

pub use service::SwanLakeEngineService;

/// Verifies that a SwanLake Flight SQL endpoint accepts and executes a query.
pub async fn probe_endpoint(endpoint: String) -> Result<(), dts_engine::EngineError> {
    let mut client = client::SwanLakeClient::connect(endpoint).await?;
    client.scalar_u64("SELECT CAST(1 AS BIGINT)".into()).await?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredTable {
    pub schema: String,
    pub name: String,
}

/// Reads a bounded list of base tables from a DuckDB/DuckLake catalog.
pub async fn discover_tables(
    endpoint: String,
    catalog: &str,
) -> Result<Vec<DiscoveredTable>, dts_engine::EngineError> {
    if catalog.trim().is_empty() || catalog.chars().any(char::is_control) {
        return Err(dts_engine::EngineError::InvalidConfig(
            "DuckDB catalog 无效".into(),
        ));
    }
    let literal = catalog.replace('\'', "''");
    let sql = format!(
        "SELECT table_schema, table_name FROM information_schema.tables \
         WHERE table_catalog='{literal}' AND table_type='BASE TABLE' \
         ORDER BY table_schema, table_name LIMIT 500"
    );
    let mut client = client::SwanLakeClient::connect(endpoint).await?;
    Ok(client
        .string_pairs(sql)
        .await?
        .into_iter()
        .map(|(schema, name)| DiscoveredTable { schema, name })
        .collect())
}
