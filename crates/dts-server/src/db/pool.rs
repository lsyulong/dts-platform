use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};
use std::str::FromStr;

/// 创建 PostgreSQL 连接池
pub async fn create_pool(database_url: &str) -> anyhow::Result<PgPool> {
    let options = PgConnectOptions::from_str(database_url)?.application_name("dts-platform");

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect_with(options)
        .await?;

    log::info!("PostgreSQL 连接池已创建");

    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    // create_pool 的集成测试需要 PostgreSQL 实例
    // 单元测试验证连接 URL 解析逻辑

    #[test]
    fn pg_connect_options_parses_valid_url() {
        let url = "postgres://user:pass@localhost:5432/dts";
        let result = PgConnectOptions::from_str(url);
        assert!(result.is_ok());
    }

    #[test]
    fn pg_connect_options_rejects_invalid_url() {
        let result = PgConnectOptions::from_str("not-a-valid-url");
        assert!(result.is_err());
    }

    #[test]
    fn pg_connect_options_with_application_name() {
        let url = "postgres://localhost/dts";
        let opts = PgConnectOptions::from_str(url)
            .unwrap()
            .application_name("dts-platform");
        assert_eq!(opts.get_application_name(), Some("dts-platform"));
    }
}
