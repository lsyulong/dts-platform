use actix_web::{HttpResponse, ResponseError};
use dts_backend::BackendError;
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
            Self::Unauthorized(msg) => write!(f, "{msg}"),
            Self::Forbidden(msg) => write!(f, "{msg}"),
            Self::NotFound(msg) => write!(f, "{msg}"),
            Self::BadRequest(msg) => write!(f, "{msg}"),
            Self::Internal(msg) => write!(f, "{msg}"),
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

impl From<BackendError> for AppError {
    fn from(err: BackendError) -> Self {
        match err {
            BackendError::NotFound(msg) => AppError::NotFound(msg),
            BackendError::AlreadyExists(msg) | BackendError::ConstraintViolation(msg) => {
                AppError::BadRequest(msg)
            },
            _ => AppError::Internal(err.to_string()),
        }
    }
}

impl From<actix_web::Error> for AppError {
    fn from(err: actix_web::Error) -> Self {
        AppError::Internal(format!("Web 服务器错误: {err}"))
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

    // ========== From<BackendError> Tests ==========

    #[test]
    fn from_backend_error_not_found() {
        let be = BackendError::NotFound("记录不存在".into());
        let app_err = AppError::from(be);
        match app_err {
            AppError::NotFound(msg) => assert_eq!(msg, "记录不存在"),
            _ => panic!("Expected NotFound"),
        }
    }

    #[test]
    fn from_backend_error_already_exists() {
        let be = BackendError::AlreadyExists("已存在".into());
        let app_err = AppError::from(be);
        match app_err {
            AppError::BadRequest(msg) => assert_eq!(msg, "已存在"),
            _ => panic!("Expected BadRequest"),
        }
    }

    #[test]
    fn from_backend_error_internal() {
        let be = BackendError::Query("查询失败".into());
        let app_err = AppError::from(be);
        match app_err {
            AppError::Internal(msg) => assert!(msg.contains("查询失败")),
            _ => panic!("Expected Internal"),
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
