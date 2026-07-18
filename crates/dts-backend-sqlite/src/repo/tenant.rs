use async_trait::async_trait;
use dts_backend::{BackendError, repo::TenantRepo, types::*};
use dts_backend_rdbms::map_sqlx_error;
use sqlx::Row;
use sqlx::SqlitePool;

pub struct SqliteTenantRepo {
    pool: SqlitePool,
}

impl SqliteTenantRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    fn row_to_info(r: &sqlx::sqlite::SqliteRow) -> TenantInfo {
        let created_at_str: String = r.get(4);
        // SQLite datetime('now') produces "YYYY-MM-DD HH:MM:SS" format (UTC).
        // Try RFC 3339 first, then fall back to SQLite format.
        let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .or_else(|_| {
                chrono::NaiveDateTime::parse_from_str(&created_at_str, "%Y-%m-%d %H:%M:%S")
                    .map(|dt| dt.and_utc())
            })
            .unwrap_or_else(|_| {
                tracing::warn!(
                    "无法解析 created_at 时间戳 '{created_at_str}'，使用当前时间作为降级值"
                );
                chrono::Utc::now()
            });
        TenantInfo {
            id: r.get(0),
            name: r.get(1),
            slug: r.get(2),
            status: r.get(3),
            created_at,
        }
    }
}

#[async_trait]
impl TenantRepo for SqliteTenantRepo {
    async fn create(&self, params: &CreateTenantParams) -> Result<String, BackendError> {
        sqlx::query("INSERT INTO tenant (id, name, slug) VALUES (?1, ?2, ?3)")
            .bind(&params.id)
            .bind(&params.name)
            .bind(&params.slug)
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        Ok(params.id.clone())
    }

    async fn find_by_slug(&self, slug: &str) -> Result<Option<TenantInfo>, BackendError> {
        let row =
            sqlx::query("SELECT id, name, slug, status, created_at FROM tenant WHERE slug = ?1")
                .bind(slug)
                .fetch_optional(&self.pool)
                .await
                .map_err(map_sqlx_error)?;
        Ok(row.as_ref().map(Self::row_to_info))
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<TenantInfo>, BackendError> {
        let row =
            sqlx::query("SELECT id, name, slug, status, created_at FROM tenant WHERE id = ?1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await
                .map_err(map_sqlx_error)?;
        Ok(row.as_ref().map(Self::row_to_info))
    }

    async fn list_all(&self) -> Result<Vec<TenantInfo>, BackendError> {
        let rows = sqlx::query(
            "SELECT id, name, slug, status, created_at FROM tenant ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(rows.iter().map(Self::row_to_info).collect())
    }

    async fn count(&self) -> Result<i64, BackendError> {
        sqlx::query_scalar("SELECT COUNT(*) FROM tenant")
            .fetch_one(&self.pool)
            .await
            .map_err(map_sqlx_error)
    }

    async fn update(
        &self,
        tenant_id: &str,
        params: &UpdateTenantParams,
    ) -> Result<(), BackendError> {
        if let Some(ref name) = params.name {
            sqlx::query("UPDATE tenant SET name = ?1 WHERE id = ?2")
                .bind(name)
                .bind(tenant_id)
                .execute(&self.pool)
                .await
                .map_err(map_sqlx_error)?;
        }
        if let Some(ref status) = params.status {
            sqlx::query("UPDATE tenant SET status = ?1 WHERE id = ?2")
                .bind(status)
                .bind(tenant_id)
                .execute(&self.pool)
                .await
                .map_err(map_sqlx_error)?;
        }
        Ok(())
    }
}
