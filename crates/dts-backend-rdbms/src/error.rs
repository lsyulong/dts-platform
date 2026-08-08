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
        },
        sqlx::Error::PoolClosed | sqlx::Error::PoolTimedOut => {
            BackendError::Connection(err.to_string())
        },
        sqlx::Error::Configuration(_) => BackendError::Configuration(err.to_string()),
        sqlx::Error::Migrate(_) => BackendError::Migration(err.to_string()),
        _ => BackendError::Internal(format!("数据库错误: {err}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_row_not_found() {
        let err = sqlx::Error::RowNotFound;
        let result = map_sqlx_error(err);
        assert!(matches!(result, BackendError::NotFound(_)));
    }

    #[test]
    fn maps_pool_closed_to_connection_error() {
        let err = sqlx::Error::PoolClosed;
        let result = map_sqlx_error(err);
        assert!(matches!(result, BackendError::Connection(_)));
    }

    #[test]
    fn maps_configuration_error() {
        let err = sqlx::Error::Configuration("bad config".into());
        let result = map_sqlx_error(err);
        assert!(matches!(result, BackendError::Configuration(_)));
    }

    #[test]
    fn maps_pool_timed_out_to_connection_error() {
        let err = sqlx::Error::PoolTimedOut;
        let result = map_sqlx_error(err);
        assert!(matches!(result, BackendError::Connection(_)));
    }

    #[test]
    fn maps_unknown_to_internal() {
        let err = sqlx::Error::Protocol("protocol error".into());
        let result = map_sqlx_error(err);
        assert!(matches!(result, BackendError::Internal(_)));
    }
}
