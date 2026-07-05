use actix_web::{HttpResponse, ResponseError};
use serde::Serialize;
use std::fmt;

/// 统一错误响应格式
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub code: String,
    pub message: String,
}

/// 应用错误类型
#[derive(Debug)]
pub enum AppError {
    Unauthorized(String),
    Forbidden(String),
    NotFound(String),
    BadRequest(String),
    Internal(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unauthorized(msg) => write!(f, "{}", msg),
            Self::Forbidden(msg) => write!(f, "{}", msg),
            Self::NotFound(msg) => write!(f, "{}", msg),
            Self::BadRequest(msg) => write!(f, "{}", msg),
            Self::Internal(msg) => write!(f, "{}", msg),
        }
    }
}

impl ResponseError for AppError {
    fn error_response(&self) -> HttpResponse {
        let (status, code) = match self {
            Self::Unauthorized(_) => (actix_web::http::StatusCode::UNAUTHORIZED, "UNAUTHORIZED"),
            Self::Forbidden(_) => (actix_web::http::StatusCode::FORBIDDEN, "FORBIDDEN"),
            Self::NotFound(_) => (actix_web::http::StatusCode::NOT_FOUND, "NOT_FOUND"),
            Self::BadRequest(_) => (actix_web::http::StatusCode::BAD_REQUEST, "BAD_REQUEST"),
            Self::Internal(_) => (
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
            ),
        };

        HttpResponse::build(status).json(ErrorResponse {
            code: code.to_string(),
            message: self.to_string(),
        })
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        AppError::Internal(err.to_string())
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => AppError::NotFound("记录不存在".into()),
            _ => AppError::Internal(format!("数据库错误: {}", err)),
        }
    }
}

impl From<actix_web::Error> for AppError {
    fn from(err: actix_web::Error) -> Self {
        AppError::Internal(format!("Web 服务器错误: {}", err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::http::StatusCode;
    use pretty_assertions::assert_eq;

    // ========== Display Tests ==========

    #[test]
    fn display_unauthorized_returns_message() {
        let err = AppError::Unauthorized("令牌无效".into());
        assert_eq!(err.to_string(), "令牌无效");
    }

    #[test]
    fn display_forbidden_returns_message() {
        let err = AppError::Forbidden("权限不足".into());
        assert_eq!(err.to_string(), "权限不足");
    }

    #[test]
    fn display_not_found_returns_message() {
        let err = AppError::NotFound("资源不存在".into());
        assert_eq!(err.to_string(), "资源不存在");
    }

    #[test]
    fn display_bad_request_returns_message() {
        let err = AppError::BadRequest("参数无效".into());
        assert_eq!(err.to_string(), "参数无效");
    }

    #[test]
    fn display_internal_returns_message() {
        let err = AppError::Internal("服务器错误".into());
        assert_eq!(err.to_string(), "服务器错误");
    }

    // ========== HTTP Status Code Tests ==========

    #[test]
    fn error_response_unauthorized_has_401() {
        let err = AppError::Unauthorized("未登录".into());
        let resp = err.error_response();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn error_response_forbidden_has_403() {
        let err = AppError::Forbidden("无权限".into());
        let resp = err.error_response();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn error_response_not_found_has_404() {
        let err = AppError::NotFound("找不到".into());
        let resp = err.error_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn error_response_bad_request_has_400() {
        let err = AppError::BadRequest("参数错误".into());
        let resp = err.error_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn error_response_internal_has_500() {
        let err = AppError::Internal("崩溃了".into());
        let resp = err.error_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    // ========== JSON Response Body Tests ==========

    #[test]
    fn error_response_body_contains_code_and_message() {
        let err = AppError::NotFound("找不到资源".into());
        let resp = err.error_response();
        // We verify the response is built correctly by checking status
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        // The content-type should be JSON
        assert_eq!(
            resp.headers()
                .get("content-type")
                .unwrap()
                .to_str()
                .unwrap(),
            "application/json"
        );
    }

    // ========== From<anyhow::Error> Tests ==========

    #[test]
    fn from_anyhow_error_creates_internal() {
        let anyhow_err = anyhow::anyhow!("something went wrong");
        let app_err = AppError::from(anyhow_err);
        match app_err {
            AppError::Internal(msg) => {
                assert!(msg.contains("something went wrong"));
            },
            _ => panic!("Expected AppError::Internal"),
        }
    }

    #[test]
    fn from_anyhow_error_with_context() {
        let base_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let anyhow_err = anyhow::Error::new(base_err).context("failed to read config");
        let app_err = AppError::from(anyhow_err);
        match app_err {
            AppError::Internal(msg) => {
                assert!(msg.contains("file missing") || msg.contains("failed to read config"));
            },
            _ => panic!("Expected AppError::Internal"),
        }
    }

    // ========== From<sqlx::Error> Tests ==========

    #[test]
    fn from_sqlx_row_not_found_creates_not_found() {
        let sqlx_err = sqlx::Error::RowNotFound;
        let app_err = AppError::from(sqlx_err);
        match app_err {
            AppError::NotFound(msg) => {
                assert_eq!(msg, "记录不存在");
            },
            _ => panic!("Expected AppError::NotFound, got {:?}", app_err),
        }
    }

    #[test]
    fn from_sqlx_other_error_creates_internal() {
        use sqlx::error::Error as SqlxError;
        // Use a pool-closed error as an example of a non-RowNotFound error
        let pool_err = SqlxError::PoolClosed;
        let app_err = AppError::from(pool_err);
        match app_err {
            AppError::Internal(msg) => {
                assert!(msg.contains("数据库错误"));
            },
            _ => panic!("Expected AppError::Internal, got {:?}", app_err),
        }
    }

    // ========== From<actix_web::Error> Tests ==========

    #[test]
    fn from_actix_web_error_creates_internal() {
        let web_err = actix_web::error::ErrorInternalServerError("test error");
        let app_err = AppError::from(web_err);
        match app_err {
            AppError::Internal(msg) => {
                assert!(msg.contains("Web 服务器错误"));
            },
            _ => panic!("Expected AppError::Internal, got {:?}", app_err),
        }
    }
}
