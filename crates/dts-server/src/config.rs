use std::env;

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
        }
    }

    /// 测试用默认配置
    pub fn for_test() -> Self {
        Self {
            database_url: "postgres://localhost:5432/dts_test".to_string(),
            database_backend: "postgres".to_string(),
            jwt_secret: "test-secret-key-for-unit-tests".to_string(),
            jwt_expiry_hours: 24,
            server_host: "127.0.0.1".to_string(),
            server_port: 0, // OS 自动分配端口
            data_dir: "./data".to_string(),
        }
    }

    /// 从环境变量加载配置
    pub fn from_env() -> anyhow::Result<Self> {
        dotenv::dotenv().ok();

        Ok(Self {
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://localhost:5432/dts".to_string()),
            database_backend: env::var("DATABASE_BACKEND")
                .unwrap_or_else(|_| "postgres".to_string()),
            jwt_secret: env::var("JWT_SECRET")
                .unwrap_or_else(|_| "dev-secret-change-me".to_string()),
            jwt_expiry_hours: env::var("JWT_EXPIRY_HOURS")
                .unwrap_or_else(|_| "24".to_string())
                .parse()
                .unwrap_or(24),
            server_host: env::var("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            server_port: env::var("SERVER_PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .unwrap_or(8080),
            data_dir: env::var("DATA_DIR").unwrap_or_else(|_| "./data".to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_config_with_given_values() {
        let config = AppConfig::new(
            "postgres://localhost/dts",
            "postgres",
            "my-secret",
            48,
            "127.0.0.1",
            3000,
            "/tmp/data",
        );
        assert_eq!(config.database_url, "postgres://localhost/dts");
        assert_eq!(config.database_backend, "postgres");
        assert_eq!(config.jwt_secret, "my-secret");
        assert_eq!(config.jwt_expiry_hours, 48);
        assert_eq!(config.server_host, "127.0.0.1");
        assert_eq!(config.server_port, 3000);
        assert_eq!(config.data_dir, "/tmp/data");
    }

    #[test]
    fn for_test_creates_postgres_config() {
        let config = AppConfig::for_test();
        assert!(config.database_url.starts_with("postgres://"));
        assert_eq!(config.database_backend, "postgres");
        assert!(config.server_port == 0);
    }

    #[test]
    fn config_is_cloneable() {
        let config = AppConfig::for_test();
        let _cloned = config.clone();
    }
}
