// Copyright 2026 DTS Platform Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::sync::Arc;
use std::time::{Duration, Instant};

use dts_backend::BackendError;
use dts_backend::repo::{
    DatasourceProbe, DatasourceRepo, RawConnectionInfo, RecordDatasourceProbeParams,
};
use dts_backend::types::*;
use sqlx::Connection;
use sqlx::Row;
use sqlx::mysql::{MySqlConnectOptions, MySqlConnection};
use sqlx::postgres::{PgConnectOptions, PgConnection};
use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection};
use uuid::Uuid;

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

struct ProbeAttempt {
    supported: bool,
    status: &'static str,
    latency_ms: Option<i64>,
    error_message: Option<&'static str>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DatasourceTable {
    pub schema: String,
    pub name: String,
}

/// 数据源管理服务
#[derive(Clone)]
pub struct DatasourceService {
    datasource_repo: Arc<dyn DatasourceRepo>,
}

impl DatasourceService {
    pub fn new(datasource_repo: Arc<dyn DatasourceRepo>) -> Self {
        Self { datasource_repo }
    }

    /// Credentials are encrypted by the protected repository before persistence.
    #[allow(clippy::too_many_arguments)]
    pub async fn create(
        &self,
        tenant_id: &str,
        name: &str,
        db_type: &str,
        host: &str,
        port: i64,
        username: Option<&str>,
        password: Option<&str>,
        database_name: Option<&str>,
        created_by: &str,
    ) -> Result<String, BackendError> {
        let kind = db_type.trim().to_ascii_lowercase();
        if name.trim().is_empty() {
            return Err(BackendError::ConstraintViolation(
                "数据源名称不能为空".into(),
            ));
        }
        if host.trim().is_empty() {
            return Err(BackendError::ConstraintViolation(
                "数据源 host 或文件路径不能为空".into(),
            ));
        }
        if kind == "sqlite" {
            if port != 0 {
                return Err(BackendError::ConstraintViolation(
                    "SQLite 数据源 port 必须为 0".into(),
                ));
            }
        } else if !(1..=65_535).contains(&port) {
            return Err(BackendError::ConstraintViolation(
                "网络数据源 port 必须在 1 到 65535 之间".into(),
            ));
        }
        if matches!(kind.as_str(), "duckdb" | "swanlake")
            && database_name.is_none_or(|catalog| catalog.trim().is_empty())
        {
            return Err(BackendError::ConstraintViolation(
                "DuckDB 数据源必须填写 DuckDB/DuckLake catalog".into(),
            ));
        }
        let id = Uuid::new_v4().to_string();
        let encrypted_password = password.map(str::to_owned);

        let params = CreateDatasourceParams {
            id: id.clone(),
            tenant_id: tenant_id.to_string(),
            name: name.to_string(),
            db_type: db_type.to_string(),
            host: host.to_string(),
            port,
            username: username.map(|s| s.to_string()),
            encrypted_password,
            database_name: database_name.map(|s| s.to_string()),
            created_by: created_by.to_string(),
        };

        self.datasource_repo.create(&params).await?;

        tracing::info!("数据源 '{name}' 创建成功");

        Ok(id)
    }

    /// 获取租户下的所有数据源
    pub async fn list_by_tenant(&self, tenant_id: &str) -> anyhow::Result<Vec<DatasourceInfo>> {
        Ok(self.datasource_repo.list_by_tenant(tenant_id).await?)
    }

    /// 检查数据源是否存在且属于该租户
    pub async fn exists(&self, id: &str, tenant_id: &str) -> anyhow::Result<bool> {
        Ok(self.datasource_repo.exists(id, tenant_id).await?)
    }

    pub async fn probe(
        &self,
        tenant_id: &str,
        datasource_id: &str,
        actor_id: &str,
    ) -> Result<DatasourceProbe, BackendError> {
        let connection = self
            .datasource_repo
            .get_connection_info(datasource_id, tenant_id)
            .await?
            .ok_or_else(|| BackendError::NotFound("数据源不存在".into()))?;
        let attempt = probe_connection(&connection, PROBE_TIMEOUT).await;
        self.datasource_repo
            .record_probe(&RecordDatasourceProbeParams {
                id: Uuid::new_v4().to_string(),
                tenant_id: tenant_id.to_owned(),
                datasource_id: datasource_id.to_owned(),
                supported: attempt.supported,
                status: attempt.status.to_owned(),
                latency_ms: attempt.latency_ms,
                error_message: attempt.error_message.map(str::to_owned),
                actor_id: actor_id.to_owned(),
                observed_at: chrono::Utc::now(),
            })
            .await
    }

    pub async fn probe_history(
        &self,
        tenant_id: &str,
        datasource_id: &str,
        limit: u32,
    ) -> Result<Vec<DatasourceProbe>, BackendError> {
        if !self
            .datasource_repo
            .exists(datasource_id, tenant_id)
            .await?
        {
            return Err(BackendError::NotFound("数据源不存在".into()));
        }
        self.datasource_repo
            .list_probes(tenant_id, datasource_id, limit)
            .await
    }

    pub async fn discover_tables(
        &self,
        tenant_id: &str,
        datasource_id: &str,
    ) -> Result<Vec<DatasourceTable>, BackendError> {
        let connection = self
            .datasource_repo
            .get_connection_info(datasource_id, tenant_id)
            .await?
            .ok_or_else(|| BackendError::NotFound("数据源不存在".into()))?;
        discover_tables(&connection, PROBE_TIMEOUT).await
    }
}

async fn discover_tables(
    connection: &RawConnectionInfo,
    timeout: Duration,
) -> Result<Vec<DatasourceTable>, BackendError> {
    let operation = async {
        match connection.db_type.trim().to_ascii_lowercase().as_str() {
            "mysql" => discover_mysql_tables(connection).await,
            "postgres" | "postgresql" | "pg" => discover_postgres_tables(connection).await,
            "duckdb" | "swanlake" => discover_swanlake_tables(connection).await,
            _ => Err(BackendError::Configuration(
                "当前数据源类型尚未实现表发现".into(),
            )),
        }
    };
    tokio::time::timeout(timeout, operation)
        .await
        .map_err(|_| BackendError::Query("数据源表发现超时".into()))?
}

async fn discover_mysql_tables(
    connection: &RawConnectionInfo,
) -> Result<Vec<DatasourceTable>, BackendError> {
    let port = u16::try_from(connection.port)
        .map_err(|_| BackendError::Configuration("MySQL port 无效".into()))?;
    let mut options = MySqlConnectOptions::new().host(&connection.host).port(port);
    if let Some(username) = connection.username.as_deref() {
        options = options.username(username);
    }
    if let Some(password) = connection.password.as_deref() {
        options = options.password(password);
    }
    if let Some(database) = connection.database_name.as_deref() {
        options = options.database(database);
    }
    let mut database = MySqlConnection::connect_with(&options)
        .await
        .map_err(|_| BackendError::Query("MySQL 表发现连接失败".into()))?;
    let rows = sqlx::query(
        "SELECT TABLE_SCHEMA, TABLE_NAME FROM information_schema.tables \
         WHERE TABLE_SCHEMA=DATABASE() AND TABLE_TYPE='BASE TABLE' \
         ORDER BY TABLE_NAME LIMIT 500",
    )
    .fetch_all(&mut database)
    .await
    .map_err(|_| BackendError::Query("MySQL 表目录读取失败".into()))?;
    rows.into_iter()
        .map(|row| {
            Ok(DatasourceTable {
                schema: row
                    .try_get("TABLE_SCHEMA")
                    .map_err(|_| BackendError::Query("MySQL 表目录响应无效".into()))?,
                name: row
                    .try_get("TABLE_NAME")
                    .map_err(|_| BackendError::Query("MySQL 表目录响应无效".into()))?,
            })
        })
        .collect()
}

async fn discover_postgres_tables(
    connection: &RawConnectionInfo,
) -> Result<Vec<DatasourceTable>, BackendError> {
    let port = u16::try_from(connection.port)
        .map_err(|_| BackendError::Configuration("PostgreSQL port 无效".into()))?;
    let mut options = PgConnectOptions::new().host(&connection.host).port(port);
    if let Some(username) = connection.username.as_deref() {
        options = options.username(username);
    }
    if let Some(password) = connection.password.as_deref() {
        options = options.password(password);
    }
    if let Some(database) = connection.database_name.as_deref() {
        options = options.database(database);
    }
    let mut database = PgConnection::connect_with(&options)
        .await
        .map_err(|_| BackendError::Query("PostgreSQL 表发现连接失败".into()))?;
    let rows = sqlx::query(
        "SELECT table_schema, table_name FROM information_schema.tables \
         WHERE table_type='BASE TABLE' AND table_schema NOT IN ('pg_catalog','information_schema') \
         ORDER BY table_schema, table_name LIMIT 500",
    )
    .fetch_all(&mut database)
    .await
    .map_err(|_| BackendError::Query("PostgreSQL 表目录读取失败".into()))?;
    rows.into_iter()
        .map(|row| {
            Ok(DatasourceTable {
                schema: row
                    .try_get("table_schema")
                    .map_err(|_| BackendError::Query("PostgreSQL 表目录响应无效".into()))?,
                name: row
                    .try_get("table_name")
                    .map_err(|_| BackendError::Query("PostgreSQL 表目录响应无效".into()))?,
            })
        })
        .collect()
}

async fn discover_swanlake_tables(
    connection: &RawConnectionInfo,
) -> Result<Vec<DatasourceTable>, BackendError> {
    let port = u16::try_from(connection.port)
        .map_err(|_| BackendError::Configuration("SwanLake port 无效".into()))?;
    let catalog = connection
        .database_name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| BackendError::Configuration("DuckDB catalog 不能为空".into()))?;
    let endpoint = if connection.host.contains("//") {
        let mut url = url::Url::parse(&connection.host)
            .map_err(|_| BackendError::Configuration("SwanLake host 无效".into()))?;
        url.set_port(Some(port))
            .map_err(|_| BackendError::Configuration("SwanLake port 无效".into()))?;
        url.to_string().trim_end_matches('/').to_owned()
    } else {
        format!("http://{}:{port}", connection.host)
    };
    dts_engine_swanlake::discover_tables(endpoint, catalog)
        .await
        .map(|tables| {
            tables
                .into_iter()
                .map(|table| DatasourceTable {
                    schema: table.schema,
                    name: table.name,
                })
                .collect()
        })
        .map_err(|_| BackendError::Query("DuckDB 表目录读取失败".into()))
}

async fn probe_connection(connection: &RawConnectionInfo, timeout: Duration) -> ProbeAttempt {
    let started = Instant::now();
    let result = match connection.db_type.trim().to_ascii_lowercase().as_str() {
        "mysql" => probe_mysql(connection, timeout).await,
        "postgres" | "postgresql" => probe_postgres(connection, timeout).await,
        "sqlite" => probe_sqlite(connection, timeout).await,
        "duckdb" | "swanlake" => probe_swanlake(connection, timeout).await,
        _ => {
            return ProbeAttempt {
                supported: false,
                status: "unknown",
                latency_ms: None,
                error_message: Some("当前数据源类型尚未实现真实连接探测"),
            };
        },
    };
    let latency_ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
    match result {
        Ok(()) => ProbeAttempt {
            supported: true,
            status: "healthy",
            latency_ms: Some(latency_ms),
            error_message: None,
        },
        Err(ProbeFailure::Timeout) => ProbeAttempt {
            supported: true,
            status: "unhealthy",
            latency_ms: Some(latency_ms),
            error_message: Some("连接探测超时"),
        },
        Err(ProbeFailure::Connection) => ProbeAttempt {
            supported: true,
            status: "unhealthy",
            latency_ms: Some(latency_ms),
            error_message: Some("连接或 SELECT 1 校验失败"),
        },
    }
}

enum ProbeFailure {
    Timeout,
    Connection,
}

async fn probe_mysql(
    connection: &RawConnectionInfo,
    duration: Duration,
) -> Result<(), ProbeFailure> {
    let port = u16::try_from(connection.port).map_err(|_| ProbeFailure::Connection)?;
    let mut options = MySqlConnectOptions::new().host(&connection.host).port(port);
    if let Some(username) = connection.username.as_deref() {
        options = options.username(username);
    }
    if let Some(password) = connection.password.as_deref() {
        options = options.password(password);
    }
    if let Some(database) = connection
        .database_name
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        options = options.database(database);
    }
    run_with_timeout(duration, async move {
        let mut conn = MySqlConnection::connect_with(&options).await?;
        sqlx::query("SELECT 1").execute(&mut conn).await?;
        Ok::<(), sqlx::Error>(())
    })
    .await
}

async fn probe_postgres(
    connection: &RawConnectionInfo,
    duration: Duration,
) -> Result<(), ProbeFailure> {
    let port = u16::try_from(connection.port).map_err(|_| ProbeFailure::Connection)?;
    let mut options = PgConnectOptions::new().host(&connection.host).port(port);
    if let Some(username) = connection.username.as_deref() {
        options = options.username(username);
    }
    if let Some(password) = connection.password.as_deref() {
        options = options.password(password);
    }
    if let Some(database) = connection
        .database_name
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        options = options.database(database);
    }
    run_with_timeout(duration, async move {
        let mut conn = PgConnection::connect_with(&options).await?;
        sqlx::query("SELECT 1").execute(&mut conn).await?;
        Ok::<(), sqlx::Error>(())
    })
    .await
}

async fn probe_sqlite(
    connection: &RawConnectionInfo,
    duration: Duration,
) -> Result<(), ProbeFailure> {
    let path = connection
        .database_name
        .as_deref()
        .filter(|value| !value.is_empty())
        .unwrap_or(&connection.host);
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(false);
    run_with_timeout(duration, async move {
        let mut conn = SqliteConnection::connect_with(&options).await?;
        sqlx::query("SELECT 1").execute(&mut conn).await?;
        Ok::<(), sqlx::Error>(())
    })
    .await
}

async fn probe_swanlake(
    connection: &RawConnectionInfo,
    duration: Duration,
) -> Result<(), ProbeFailure> {
    let port = u16::try_from(connection.port).map_err(|_| ProbeFailure::Connection)?;
    if port == 0 || connection.host.trim().is_empty() {
        return Err(ProbeFailure::Connection);
    }
    let endpoint = if connection.host.contains("//") {
        let mut url = url::Url::parse(&connection.host).map_err(|_| ProbeFailure::Connection)?;
        url.set_port(Some(port))
            .map_err(|_| ProbeFailure::Connection)?;
        url.to_string().trim_end_matches('/').to_owned()
    } else {
        format!("http://{}:{port}", connection.host)
    };
    match tokio::time::timeout(duration, dts_engine_swanlake::probe_endpoint(endpoint)).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(_)) => Err(ProbeFailure::Connection),
        Err(_) => Err(ProbeFailure::Timeout),
    }
}

async fn run_with_timeout<F>(duration: Duration, future: F) -> Result<(), ProbeFailure>
where
    F: std::future::Future<Output = Result<(), sqlx::Error>>,
{
    match tokio::time::timeout(duration, future).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(_)) => Err(ProbeFailure::Connection),
        Err(_) => Err(ProbeFailure::Timeout),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use dts_backend::BackendError;

    #[tokio::test]
    async fn sqlite_memory_probe_executes_select_one() {
        let attempt = probe_connection(
            &RawConnectionInfo {
                db_type: "SQLite".into(),
                host: ":memory:".into(),
                port: 0,
                username: None,
                password: None,
                database_name: None,
            },
            Duration::from_secs(1),
        )
        .await;
        assert!(attempt.supported);
        assert_eq!(attempt.status, "healthy");
        assert!(attempt.error_message.is_none());
    }

    #[tokio::test]
    async fn unsupported_probe_is_explicitly_unknown() {
        let attempt = probe_connection(
            &RawConnectionInfo {
                db_type: "MongoDB".into(),
                host: "localhost".into(),
                port: 27017,
                username: None,
                password: None,
                database_name: None,
            },
            Duration::from_secs(1),
        )
        .await;
        assert!(!attempt.supported);
        assert_eq!(attempt.status, "unknown");
        assert_eq!(
            attempt.error_message,
            Some("当前数据源类型尚未实现真实连接探测")
        );
    }

    #[tokio::test]
    async fn failed_probe_does_not_expose_connection_details() {
        let secret_path = "Z:/private/secret-does-not-exist/database.sqlite";
        let attempt = probe_connection(
            &RawConnectionInfo {
                db_type: "SQLite".into(),
                host: secret_path.into(),
                port: 0,
                username: None,
                password: Some("top-secret".into()),
                database_name: None,
            },
            Duration::from_secs(1),
        )
        .await;
        assert!(attempt.supported);
        assert_eq!(attempt.status, "unhealthy");
        let message = attempt.error_message.unwrap_or_default();
        assert!(!message.contains(secret_path));
        assert!(!message.contains("top-secret"));
    }

    /// 手动 mock DatasourceRepo — 捕获 create 调用参数，返回预设结果
    struct MockDatasourceRepo {
        last_create_params: Mutex<Option<CreateDatasourceParams>>,
        exists_result: Mutex<bool>,
    }

    impl MockDatasourceRepo {
        fn new() -> Self {
            Self {
                last_create_params: Mutex::new(None),
                exists_result: Mutex::new(false),
            }
        }

        fn set_exists_result(&self, value: bool) {
            *self.exists_result.lock().expect("Mutex poisoned") = value;
        }

        fn last_create_params(&self) -> Option<CreateDatasourceParams> {
            self.last_create_params
                .lock()
                .expect("Mutex poisoned")
                .clone()
        }
    }

    #[async_trait]
    impl DatasourceRepo for MockDatasourceRepo {
        async fn create(&self, params: &CreateDatasourceParams) -> Result<String, BackendError> {
            *self.last_create_params.lock().expect("Mutex poisoned") = Some(params.clone());
            Ok(params.id.clone())
        }

        async fn list_by_tenant(
            &self,
            _tenant_id: &str,
        ) -> Result<Vec<DatasourceInfo>, BackendError> {
            Ok(vec![])
        }

        async fn find_detail_by_id(
            &self,
            _id: &str,
            _tenant_id: &str,
        ) -> Result<Option<DatasourceDetail>, BackendError> {
            Ok(None)
        }

        async fn delete(&self, _id: &str, _tenant_id: &str) -> Result<bool, BackendError> {
            Ok(true)
        }

        async fn get_connection_info(
            &self,
            _id: &str,
            _tenant_id: &str,
        ) -> Result<Option<dts_backend::repo::RawConnectionInfo>, BackendError> {
            Ok(None)
        }

        async fn count_by_tenant(&self, _tenant_id: &str) -> Result<i64, BackendError> {
            Ok(0)
        }

        async fn count_healthy_by_tenant(&self, _tenant_id: &str) -> Result<i64, BackendError> {
            Ok(0)
        }

        async fn exists(&self, _id: &str, _tenant_id: &str) -> Result<bool, BackendError> {
            Ok(*self.exists_result.lock().expect("Mutex poisoned"))
        }
    }

    /// The repository receives the original password, including any literal prefix.
    #[tokio::test]
    async fn create_passes_original_password_to_protected_repository() {
        let repo = Arc::new(MockDatasourceRepo::new());

        let svc = DatasourceService::new(repo.clone());
        let result = svc
            .create(
                "tenant-1",
                "mysql-db",
                "mysql",
                "localhost",
                3306,
                Some("root"),
                Some("secret123"),
                Some("mydb"),
                "admin-user",
            )
            .await;

        assert!(result.is_ok());

        let params = repo
            .last_create_params()
            .expect("create should have been called");
        assert_eq!(
            params.encrypted_password.as_deref(),
            Some("secret123"),
            "credential protection belongs to the repository"
        );
    }

    /// 验证无密码时 encrypted_password 为 None
    #[tokio::test]
    async fn create_sets_encrypted_password_to_none_when_password_absent() {
        let repo = Arc::new(MockDatasourceRepo::new());

        let svc = DatasourceService::new(repo.clone());
        let result = svc
            .create(
                "tenant-1",
                "pg-db",
                "postgresql",
                "db.example.com",
                5432,
                None,
                None,
                None,
                "admin-user",
            )
            .await;

        assert!(result.is_ok());

        let params = repo
            .last_create_params()
            .expect("create should have been called");
        assert!(
            params.encrypted_password.is_none(),
            "encrypted_password should be None when no password provided"
        );
    }

    /// 验证 create 返回 UUID v4 格式的 ID
    #[tokio::test]
    async fn create_returns_non_empty_id() {
        let repo = Arc::new(MockDatasourceRepo::new());

        let svc = DatasourceService::new(repo.clone());
        let result = svc
            .create(
                "tenant-1",
                "pg-db",
                "postgresql",
                "db.example.com",
                5432,
                None,
                None,
                None,
                "admin-user",
            )
            .await;

        assert!(result.is_ok());
        let id = result.unwrap();
        assert!(!id.is_empty());
        // UUID v4 格式: 36 字符 + 4 个连字符
        assert_eq!(id.len(), 36);
        assert_eq!(id.chars().filter(|c| *c == '-').count(), 4);
    }

    #[tokio::test]
    async fn create_rejects_duckdb_without_catalog() {
        let repo = Arc::new(MockDatasourceRepo::new());
        let svc = DatasourceService::new(repo.clone());
        let result = svc
            .create(
                "tenant-1",
                "warehouse",
                "DuckDB",
                "swanlake.internal",
                4214,
                None,
                None,
                None,
                "admin-user",
            )
            .await;
        assert!(result.is_err());
        assert!(repo.last_create_params().is_none());
    }

    /// 验证 list_by_tenant 正确委托给 repo
    #[tokio::test]
    async fn list_by_tenant_delegates_to_repo() {
        let repo = Arc::new(MockDatasourceRepo::new());

        let svc = DatasourceService::new(repo.clone());
        let result = svc.list_by_tenant("tenant-1").await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    /// 验证 exists 正确委托给 repo（存在时返回 true）
    #[tokio::test]
    async fn exists_delegates_to_repo() {
        let repo = Arc::new(MockDatasourceRepo::new());
        repo.set_exists_result(true);

        let svc = DatasourceService::new(repo.clone());
        let result = svc.exists("ds-1", "tenant-1").await;

        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    /// 验证 exists 返回 false（数据源不存在）
    #[tokio::test]
    async fn exists_returns_false_when_not_found() {
        let repo = Arc::new(MockDatasourceRepo::new());
        // exists_result 默认为 false

        let svc = DatasourceService::new(repo.clone());
        let result = svc.exists("ds-missing", "tenant-1").await;

        assert!(result.is_ok());
        assert!(!result.unwrap());
    }
}
