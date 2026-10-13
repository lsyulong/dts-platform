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

use std::env;

/// Default values used when environment variables are not set
pub mod defaults {
    pub const DATABASE_URL: &str = "sqlite://data/dts.db?mode=rwc";
    pub const DATABASE_BACKEND: &str = "sqlite";
    pub const AUTH_TOKEN_TIMEOUT_SECS: i64 = 86_400;
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
#[derive(Clone)]
pub struct SchedulerConfig {
    pub enabled: bool,
    pub base_url: String,
    pub username: String,
    pub password: String,
    pub callback_base_url: String,
    pub instance_id: String,
    /// 用于 jiascheduler 内部触发的 HMAC 密钥（独立于用户会话）
    pub service_token: String,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: "http://localhost:9090".to_string(),
            username: "admin".to_string(),
            password: "admin".to_string(),
            callback_base_url: String::new(),
            instance_id: String::new(),
            service_token: "dev-service-token-change-me".to_string(),
        }
    }
}

impl std::fmt::Debug for SchedulerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SchedulerConfig")
            .field("enabled", &self.enabled)
            .field("base_url", &self.base_url)
            .field("username", &self.username)
            .field("password", &"<REDACTED>")
            .field("callback_base_url", &self.callback_base_url)
            .field("instance_id", &self.instance_id)
            .field("service_token", &"<REDACTED>")
            .finish()
    }
}

impl SchedulerConfig {
    fn callback_url(&self) -> anyhow::Result<reqwest::Url> {
        let url = reqwest::Url::parse(&self.callback_base_url)
            .map_err(|_| anyhow::anyhow!("请配置 worker 可访问的 JIA_CALLBACK_BASE_URL"))?;
        if !["http", "https"].contains(&url.scheme())
            || url.host_str().is_none()
            || matches!(url.host_str(), Some("0.0.0.0" | "[::]" | "::"))
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            anyhow::bail!(
                "JIA_CALLBACK_BASE_URL 必须为绝对 HTTP(S) URL，不能含通配地址、认证信息、query 或 fragment"
            );
        }
        Ok(url)
    }

    pub fn trigger_url(
        &self,
        task_id: &str,
        tenant_id: &str,
        actor_id: &str,
    ) -> anyhow::Result<String> {
        let mut url = self.callback_url()?;
        url.path_segments_mut()
            .map_err(|_| anyhow::anyhow!("回调 URL 路径无效"))?
            .pop_if_empty()
            .extend(["api", "v1", "internal", "tasks", task_id, "start"]);
        url.query_pairs_mut()
            .append_pair("tenant_id", tenant_id)
            .append_pair("actor_id", actor_id);
        Ok(url.into())
    }

    pub fn workflow_trigger_url(
        &self,
        workflow_id: &str,
        tenant_id: &str,
        actor_id: &str,
        plan_version: u64,
    ) -> anyhow::Result<String> {
        let mut url = self.callback_url()?;
        url.path_segments_mut()
            .map_err(|_| anyhow::anyhow!("回调 URL 路径无效"))?
            .pop_if_empty()
            .extend([
                "api",
                "v1",
                "internal",
                "development",
                "workflows",
                workflow_id,
                "trigger",
            ]);
        url.query_pairs_mut()
            .append_pair("tenant_id", tenant_id)
            .append_pair("actor_id", actor_id)
            .append_pair("plan_version", &plan_version.to_string());
        Ok(url.into())
    }

    pub fn validate_instance(&self) -> anyhow::Result<()> {
        if self.instance_id.is_empty()
            || self.instance_id.len() > 128
            || !self
                .instance_id
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || "-_.:".contains(ch))
        {
            anyhow::bail!("请配置单个 JIA_INSTANCE_ID（运行 bash 和 curl 的 Linux worker）");
        }
        Ok(())
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

/// SwanLake 分析引擎配置 (DuckDB + Arrow Flight SQL)
#[derive(Clone, Debug)]
pub struct SwanLakeConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub tenant_slug: String,
}

impl Default for SwanLakeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            host: "localhost".to_string(),
            port: 4214,
            tenant_slug: "default".into(),
        }
    }
}

impl SwanLakeConfig {
    /// gRPC 连接 URI
    pub fn uri(&self) -> String {
        format!("grpc://{}:{}", self.host, self.port)
    }
}

#[derive(Clone, Debug)]
pub struct AlertAutomationConfig {
    pub enabled: bool,
    pub poll_seconds: u64,
    pub lease_seconds: u32,
    pub max_attempts: u32,
    pub retry_base_seconds: u64,
}

impl Default for AlertAutomationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            poll_seconds: 5,
            lease_seconds: 30,
            max_attempts: 5,
            retry_base_seconds: 30,
        }
    }
}

impl AlertAutomationConfig {
    fn from_env() -> anyhow::Result<Self> {
        let parse = |name: &str, default: u64| -> anyhow::Result<u64> {
            env::var(name)
                .unwrap_or_else(|_| default.to_string())
                .parse()
                .map_err(|_| anyhow::anyhow!("{name} 必须为正整数"))
        };
        let config = Self {
            enabled: env::var("ALERT_AUTOMATION_ENABLED")
                .map(|value| value == "true" || value == "1")
                .unwrap_or(false),
            poll_seconds: parse("ALERT_AUTOMATION_POLL_SECONDS", 5)?,
            lease_seconds: u32::try_from(parse("ALERT_AUTOMATION_LEASE_SECONDS", 30)?)
                .map_err(|_| anyhow::anyhow!("ALERT_AUTOMATION_LEASE_SECONDS 超出范围"))?,
            max_attempts: u32::try_from(parse("ALERT_AUTOMATION_MAX_ATTEMPTS", 5)?)
                .map_err(|_| anyhow::anyhow!("ALERT_AUTOMATION_MAX_ATTEMPTS 超出范围"))?,
            retry_base_seconds: parse("ALERT_AUTOMATION_RETRY_BASE_SECONDS", 30)?,
        };
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if !(1..=300).contains(&self.poll_seconds)
            || !(5..=300).contains(&self.lease_seconds)
            || !(1..=100).contains(&self.max_attempts)
            || !(1..=3600).contains(&self.retry_base_seconds)
        {
            anyhow::bail!("告警自动化参数超出允许范围");
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct AuthConfig {
    pub storage_backend: String,
    pub redis_url: Option<String>,
    pub token_timeout_secs: i64,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            storage_backend: "memory".into(),
            redis_url: None,
            token_timeout_secs: defaults::AUTH_TOKEN_TIMEOUT_SECS,
        }
    }
}

impl std::fmt::Debug for AuthConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthConfig")
            .field("storage_backend", &self.storage_backend)
            .field("redis_url", &self.redis_url.as_ref().map(|_| "<REDACTED>"))
            .field("token_timeout_secs", &self.token_timeout_secs)
            .finish()
    }
}

impl AuthConfig {
    fn from_env() -> anyhow::Result<Self> {
        let config = Self {
            storage_backend: env::var("AUTH_STORAGE").unwrap_or_else(|_| "memory".into()),
            redis_url: env::var("AUTH_REDIS_URL").ok(),
            token_timeout_secs: env::var("AUTH_TOKEN_TIMEOUT_SECS")
                .unwrap_or_else(|_| defaults::AUTH_TOKEN_TIMEOUT_SECS.to_string())
                .parse()
                .map_err(|_| anyhow::anyhow!("AUTH_TOKEN_TIMEOUT_SECS 必须为整数秒"))?,
        };
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if !(1..=31_536_000).contains(&self.token_timeout_secs) {
            anyhow::bail!("AUTH_TOKEN_TIMEOUT_SECS 必须介于 1 与 31536000 秒之间");
        }
        match self.storage_backend.as_str() {
            "memory" => Ok(()),
            "redis" => {
                self.redis_url
                    .as_deref()
                    .and_then(|value| reqwest::Url::parse(value).ok())
                    .filter(|url| {
                        ["redis", "rediss"].contains(&url.scheme())
                            && url.host_str().is_some()
                            && url.fragment().is_none()
                    })
                    .ok_or_else(|| {
                        anyhow::anyhow!("请配置有效 AUTH_REDIS_URL（redis:// 或 rediss://）")
                    })?;
                Ok(())
            },
            _ => anyhow::bail!("AUTH_STORAGE 必须为 memory 或 redis"),
        }
    }
}

/// 应用配置，通过环境变量或手动构造加载
#[derive(Clone, Debug)]
pub struct AppConfig {
    pub database_url: String,
    pub database_backend: String,
    pub auth: AuthConfig,
    pub server_host: String,
    pub server_port: u16,
    pub data_dir: String,
    pub scheduler: SchedulerConfig,
    pub deerflow: DeerFlowConfig,
    pub swanlake: SwanLakeConfig,
    pub alert_automation: AlertAutomationConfig,
}

impl AppConfig {
    /// 手动构造配置（用于测试注入）
    pub fn new(
        database_url: impl Into<String>,
        database_backend: impl Into<String>,
        server_host: impl Into<String>,
        server_port: u16,
        data_dir: impl Into<String>,
    ) -> Self {
        Self {
            database_url: database_url.into(),
            database_backend: database_backend.into(),
            auth: AuthConfig::default(),
            server_host: server_host.into(),
            server_port,
            data_dir: data_dir.into(),
            scheduler: SchedulerConfig::default(),
            deerflow: DeerFlowConfig::default(),
            swanlake: SwanLakeConfig::default(),
            alert_automation: AlertAutomationConfig::default(),
        }
    }

    /// 测试用默认配置（SQLite 内存模式）
    pub fn for_test() -> Self {
        Self {
            database_url: "sqlite::memory:".to_string(),
            database_backend: defaults::DATABASE_BACKEND.to_string(),
            auth: AuthConfig::default(),
            server_host: "127.0.0.1".to_string(),
            server_port: 0,
            data_dir: defaults::DATA_DIR.to_string(),
            scheduler: SchedulerConfig::default(),
            deerflow: DeerFlowConfig::default(),
            swanlake: SwanLakeConfig::default(),
            alert_automation: AlertAutomationConfig::default(),
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
            auth: AuthConfig::from_env()?,
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
                callback_base_url: env::var("JIA_CALLBACK_BASE_URL")
                    .or_else(|_| env::var("JIA_CALLBACK_URL"))
                    .unwrap_or_default(),
                instance_id: env::var("JIA_INSTANCE_ID").unwrap_or_default(),
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
            swanlake: SwanLakeConfig {
                enabled: env::var("SWANLAKE_ENABLED")
                    .map(|v| v == "true" || v == "1")
                    .unwrap_or(false),
                host: env::var("SWANLAKE_HOST").unwrap_or_else(|_| "localhost".into()),
                port: env::var("SWANLAKE_PORT")
                    .unwrap_or_else(|_| "4214".into())
                    .parse()
                    .unwrap_or(4214),
                tenant_slug: env::var("SWANLAKE_TENANT_SLUG").unwrap_or_else(|_| "default".into()),
            },
            alert_automation: AlertAutomationConfig::from_env()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_configuration_validates_ttl_and_redacts_redis_credentials() {
        let mut config = AuthConfig::default();
        assert!(config.validate().is_ok());
        config.token_timeout_secs = 0;
        assert!(config.validate().is_err());
        config.token_timeout_secs = 86_400;
        config.storage_backend = "redis".into();
        assert!(config.validate().is_err());
        config.redis_url = Some("redis://:fixture-redis-password@localhost:6379/0".into());
        assert!(config.validate().is_ok());
        assert!(!format!("{config:?}").contains("fixture-redis-password"));
        config.redis_url = Some("http://localhost:6379".into());
        assert!(config.validate().is_err());
        config.storage_backend = "unknown".into();
        assert!(config.validate().is_err());
    }

    #[test]
    fn scheduler_callback_requires_explicit_safe_url_and_encodes_scopes() {
        let mut config = SchedulerConfig::default();
        for invalid in [
            "",
            "/relative",
            "http://0.0.0.0:8080",
            "http://[::]:8080",
            "http://user:pass@host",
            "https://host?token=x",
            "https://host/#fragment",
        ] {
            config.callback_base_url = invalid.into();
            assert!(
                config.trigger_url("task", "tenant", "actor").is_err(),
                "{invalid}"
            );
        }
        config.callback_base_url = "https://platform.example/prefix/".into();
        let url = reqwest::Url::parse(
            &config
                .trigger_url("task", "t&foreign=1", "actor+1")
                .unwrap(),
        )
        .unwrap();
        assert_eq!(url.path(), "/prefix/api/v1/internal/tasks/task/start");
        assert_eq!(
            url.query_pairs()
                .map(|(key, value)| (key.into_owned(), value.into_owned()))
                .collect::<Vec<_>>(),
            [
                ("tenant_id".to_owned(), "t&foreign=1".to_owned()),
                ("actor_id".to_owned(), "actor+1".to_owned())
            ]
        );
        let workflow_url = reqwest::Url::parse(
            &config
                .workflow_trigger_url("workflow", "tenant", "actor", 7)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            workflow_url.path(),
            "/prefix/api/v1/internal/development/workflows/workflow/trigger"
        );
        assert_eq!(
            workflow_url
                .query_pairs()
                .map(|(key, value)| (key.into_owned(), value.into_owned()))
                .collect::<Vec<_>>(),
            [
                ("tenant_id".to_owned(), "tenant".to_owned()),
                ("actor_id".to_owned(), "actor".to_owned()),
                ("plan_version".to_owned(), "7".to_owned()),
            ]
        );
        assert!(config.validate_instance().is_err());
        config.instance_id = "one,two".into();
        assert!(config.validate_instance().is_err());
        config.instance_id = "worker-1".into();
        assert!(config.validate_instance().is_ok());
    }

    #[test]
    fn scheduler_debug_redacts_password_and_signing_secret() {
        let config = SchedulerConfig {
            password: "fixture-password".into(),
            service_token: "fixture-service-signing-secret".into(),
            ..SchedulerConfig::default()
        };
        let debug = format!("{config:?}");
        assert!(!debug.contains("fixture-password"));
        assert!(!debug.contains("fixture-service-signing-secret"));
    }

    #[test]
    fn new_creates_config_with_given_values() {
        let config = AppConfig::new(
            "sqlite://data/dts.db?mode=rwc",
            "sqlite",
            "127.0.0.1",
            3000,
            "/tmp/data",
        );
        assert_eq!(config.database_url, "sqlite://data/dts.db?mode=rwc");
        assert_eq!(config.database_backend, "sqlite");
        assert_eq!(config.auth.storage_backend, "memory");
        assert_eq!(config.auth.token_timeout_secs, 86_400);
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
        let _cloned = config;
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
