use std::env;

/// Default values used when environment variables are not set
pub mod defaults {
    pub const DATABASE_URL: &str = "sqlite://data/dts.db?mode=rwc";
    pub const DATABASE_BACKEND: &str = "sqlite";
    pub const JWT_SECRET: &str = "dev-secret-change-me";
    pub const JWT_EXPIRY_HOURS: i64 = 24;
    pub const SERVER_HOST: &str = "0.0.0.0";
    pub const SERVER_PORT: u16 = 8080;
    pub const DATA_DIR: &str = "./data";
}

/// Valid task status values in allowed transition order
pub const VALID_TASK_STATUSES: &[&str] = &["draft", "running", "paused", "completed", "failed"];

/// Valid alert metric types
pub const VALID_METRIC_TYPES: &[&str] = &["sync_delay", "error_count", "row_diff", "bytes_diff"];

/// Valid alert comparison operators
pub const VALID_ALERT_OPERATORS: &[&str] = &["gt", "gte", "lt", "lte", "eq"];

/// jiascheduler 集成配置
#[derive(Clone, Debug)]
pub struct SchedulerConfig {
    pub enabled: bool,
    pub base_url: String,
    pub username: String,
    pub password: String,
    pub callback_base_url: String,
    /// 用于 jiascheduler callback 认证的服务 token（独立于 JWT secret）
    pub service_token: String,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: "http://localhost:9090".to_string(),
            username: "admin".to_string(),
            password: "admin".to_string(),
            callback_base_url: "http://localhost:8080".to_string(),
            service_token: "dev-service-token-change-me".to_string(),
        }
    }
}

/// DeerFlow AI 集成配置
#[derive(Clone, Debug)]
pub struct DeerFlowConfig {
    pub enabled: bool,
    pub base_url: String,
    pub api_key: String,
}

impl Default for DeerFlowConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: "http://localhost:11434".to_string(),
            api_key: String::new(),
        }
    }
}

/// 应用配置，通过环境变量或手动构造加载
#[derive(Clone, Debug)]
pub struct AppConfig {
    pub database_url: String,
    pub database_backend: String,
    pub jwt_secret: String,
    pub jwt_expiry_hours: i64,
    pub server_host: String,
    pub server_port: u16,
    pub data_dir: String,
    pub scheduler: SchedulerConfig,
    pub deerflow: DeerFlowConfig,
}

impl AppConfig {
    /// 手动构造配置（用于测试注入）
    pub fn new(
        database_url: impl Into<String>,
        database_backend: impl Into<String>,
        jwt_secret: impl Into<String>,
        jwt_expiry_hours: i64,
        server_host: impl Into<String>,
        server_port: u16,
        data_dir: impl Into<String>,
    ) -> Self {
        Self {
            database_url: database_url.into(),
            database_backend: database_backend.into(),
            jwt_secret: jwt_secret.into(),
            jwt_expiry_hours,
            server_host: server_host.into(),
            server_port,
            data_dir: data_dir.into(),
            scheduler: SchedulerConfig::default(),
            deerflow: DeerFlowConfig::default(),
        }
    }

    /// 测试用默认配置（SQLite 内存模式）
    pub fn for_test() -> Self {
        Self {
            database_url: "sqlite::memory:".to_string(),
            database_backend: defaults::DATABASE_BACKEND.to_string(),
            jwt_secret: "test-secret-key-for-unit-tests".to_string(),
            jwt_expiry_hours: defaults::JWT_EXPIRY_HOURS,
            server_host: "127.0.0.1".to_string(),
            server_port: 0,
            data_dir: defaults::DATA_DIR.to_string(),
            scheduler: SchedulerConfig::default(),
            deerflow: DeerFlowConfig::default(),
        }
    }

    /// 从环境变量加载配置
    pub fn from_env() -> anyhow::Result<Self> {
        dotenv::dotenv().ok();

        Ok(Self {
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| defaults::DATABASE_URL.to_string()),
            database_backend: env::var("DATABASE_BACKEND")
                .unwrap_or_else(|_| defaults::DATABASE_BACKEND.to_string()),
            jwt_secret: env::var("JWT_SECRET").unwrap_or_else(|_| defaults::JWT_SECRET.to_string()),
            jwt_expiry_hours: env::var("JWT_EXPIRY_HOURS")
                .unwrap_or_else(|_| defaults::JWT_EXPIRY_HOURS.to_string())
                .parse()
                .unwrap_or(defaults::JWT_EXPIRY_HOURS),
            server_host: env::var("SERVER_HOST")
                .unwrap_or_else(|_| defaults::SERVER_HOST.to_string()),
            server_port: env::var("SERVER_PORT")
                .unwrap_or_else(|_| defaults::SERVER_PORT.to_string())
                .parse()
                .unwrap_or(defaults::SERVER_PORT),
            data_dir: env::var("DATA_DIR").unwrap_or_else(|_| defaults::DATA_DIR.to_string()),
            scheduler: SchedulerConfig {
                enabled: env::var("JIA_ENABLED")
                    .map(|v| v == "true" || v == "1")
                    .unwrap_or(false),
                base_url: env::var("JIA_BASE_URL")
                    .unwrap_or_else(|_| "http://localhost:9090".to_string()),
                username: env::var("JIA_USERNAME").unwrap_or_else(|_| "admin".to_string()),
                password: env::var("JIA_PASSWORD").unwrap_or_else(|_| "admin".to_string()),
                callback_base_url: env::var("JIA_CALLBACK_URL").unwrap_or_else(|_| {
                    format!("http://{}:{}", defaults::SERVER_HOST, defaults::SERVER_PORT)
                }),
                service_token: env::var("JIA_SERVICE_TOKEN")
                    .unwrap_or_else(|_| "dev-service-token-change-me".to_string()),
            },
            deerflow: DeerFlowConfig {
                enabled: env::var("DEERFLOW_ENABLED")
                    .map(|v| v == "true" || v == "1")
                    .unwrap_or(false),
                base_url: env::var("DEERFLOW_BASE_URL")
                    .unwrap_or_else(|_| "http://localhost:11434".to_string()),
                api_key: env::var("DEERFLOW_API_KEY").unwrap_or_default(),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_config_with_given_values() {
        let config = AppConfig::new(
            "sqlite://data/dts.db?mode=rwc",
            "sqlite",
            "my-secret",
            48,
            "127.0.0.1",
            3000,
            "/tmp/data",
        );
        assert_eq!(config.database_url, "sqlite://data/dts.db?mode=rwc");
        assert_eq!(config.database_backend, "sqlite");
        assert_eq!(config.jwt_secret, "my-secret");
        assert_eq!(config.jwt_expiry_hours, 48);
        assert_eq!(config.server_host, "127.0.0.1");
        assert_eq!(config.server_port, 3000);
        assert_eq!(config.data_dir, "/tmp/data");
    }

    #[test]
    fn for_test_creates_sqlite_config() {
        let config = AppConfig::for_test();
        assert_eq!(config.database_url, "sqlite::memory:");
        assert_eq!(config.database_backend, "sqlite");
        assert!(config.server_port == 0);
    }

    #[test]
    fn config_is_cloneable() {
        let config = AppConfig::for_test();
        let _cloned = config.clone();
    }

    #[test]
    fn defaults_are_non_empty() {
        assert!(!defaults::DATABASE_URL.is_empty());
        assert!(!defaults::DATABASE_BACKEND.is_empty());
        assert!(!defaults::JWT_SECRET.is_empty());
        assert!(defaults::JWT_EXPIRY_HOURS > 0);
        assert!(!defaults::SERVER_HOST.is_empty());
        assert!(defaults::SERVER_PORT > 0);
        assert!(!defaults::DATA_DIR.is_empty());
    }

    #[test]
    fn valid_task_statuses_contains_expected_values() {
        assert!(VALID_TASK_STATUSES.contains(&"draft"));
        assert!(VALID_TASK_STATUSES.contains(&"running"));
        assert!(VALID_TASK_STATUSES.contains(&"paused"));
        assert!(VALID_TASK_STATUSES.contains(&"completed"));
        assert!(VALID_TASK_STATUSES.contains(&"failed"));
    }

    #[test]
    fn valid_metric_types_contains_expected_values() {
        assert!(VALID_METRIC_TYPES.contains(&"sync_delay"));
        assert!(VALID_METRIC_TYPES.contains(&"error_count"));
        assert!(VALID_METRIC_TYPES.contains(&"row_diff"));
        assert!(VALID_METRIC_TYPES.contains(&"bytes_diff"));
    }

    #[test]
    fn valid_alert_operators_contains_expected_values() {
        assert!(VALID_ALERT_OPERATORS.contains(&"gt"));
        assert!(VALID_ALERT_OPERATORS.contains(&"gte"));
        assert!(VALID_ALERT_OPERATORS.contains(&"lt"));
        assert!(VALID_ALERT_OPERATORS.contains(&"lte"));
        assert!(VALID_ALERT_OPERATORS.contains(&"eq"));
    }
}
