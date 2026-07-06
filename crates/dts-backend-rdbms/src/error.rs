use dts_backend::BackendError;

/// 将 sqlx 错误映射为 BackendError
///
/// 所有基于 sqlx 的 RDBMS 实现共用此函数。
pub fn map_sqlx_error(err: sqlx::Error) -> BackendError {
    match err {
        sqlx::Error::RowNotFound => BackendError::NotFound("记录不存在".into()),
        sqlx::Error::Database(db_err) => {
            let msg = db_err.to_string();
            if let Some(code) = db_err.code() {
                match code.as_ref() {
                    // 唯一约束冲突 (PostgreSQL: 23505, MySQL: 1062)
                    "23505" | "1062" => BackendError::AlreadyExists(msg),
                    // 外键约束冲突 (PostgreSQL: 23503, MySQL: 1452)
                    "23503" | "1452" => BackendError::ConstraintViolation(msg),
                    _ => BackendError::Query(msg),
                }
            } else {
                BackendError::Query(msg)
            }
        }
        sqlx::Error::PoolClosed | sqlx::Error::PoolTimedOut => {
            BackendError::Connection(err.to_string())
        }
        sqlx::Error::Configuration(_) => BackendError::Configuration(err.to_string()),
        sqlx::Error::Migrate(_) => BackendError::Migration(err.to_string()),
        _ => BackendError::Internal(format!("数据库错误: {err}")),
    }
}
