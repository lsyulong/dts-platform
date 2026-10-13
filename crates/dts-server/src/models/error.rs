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
    Conflict(String),
    RateLimited { retry_after_secs: u32 },
    Internal(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unauthorized(msg) => write!(f, "{msg}"),
            Self::Forbidden(msg) => write!(f, "{msg}"),
            Self::NotFound(msg) => write!(f, "{msg}"),
            Self::BadRequest(msg) => write!(f, "{msg}"),
            Self::Conflict(msg) => write!(f, "{msg}"),
            Self::RateLimited { .. } => write!(f, "调用次数超过每分钟上限，请稍后重试"),
            Self::Internal(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for AppError {}

impl ResponseError for AppError {
    fn status_code(&self) -> actix_web::http::StatusCode {
        use actix_web::http::StatusCode;
        match self {
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::RateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
    fn error_response(&self) -> HttpResponse {
        let (status, code) = match self {
            Self::Unauthorized(_) => (actix_web::http::StatusCode::UNAUTHORIZED, "UNAUTHORIZED"),
            Self::Forbidden(_) => (actix_web::http::StatusCode::FORBIDDEN, "FORBIDDEN"),
            Self::NotFound(_) => (actix_web::http::StatusCode::NOT_FOUND, "NOT_FOUND"),
            Self::BadRequest(_) => (actix_web::http::StatusCode::BAD_REQUEST, "BAD_REQUEST"),
            Self::Conflict(_) => (actix_web::http::StatusCode::CONFLICT, "CONFLICT"),
            Self::RateLimited { .. } => (
                actix_web::http::StatusCode::TOO_MANY_REQUESTS,
                "RATE_LIMITED",
            ),
            Self::Internal(_) => (
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
            ),
        };

        let message = match self {
            Self::Internal(_) => "服务内部错误，请查看服务日志".to_string(),
            _ => self.to_string(),
        };
        let mut response = HttpResponse::build(status);
        if let Self::RateLimited { retry_after_secs } = self {
            response.insert_header(("Retry-After", retry_after_secs.max(&1).to_string()));
        }
        response.json(ErrorResponse {
            code: code.to_string(),
            message,
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
            BackendError::Conflict(msg) => AppError::Conflict(msg),
            BackendError::Forbidden(msg) => AppError::Forbidden(msg),
            BackendError::RateLimited { retry_after_secs } => {
                AppError::RateLimited { retry_after_secs }
            },
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

    #[test]
    fn rate_limit_has_429_and_the_actual_retry_after_header() {
        let error = AppError::from(BackendError::RateLimited {
            retry_after_secs: 17,
        });
        let response = error.error_response();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            response
                .headers()
                .get("Retry-After")
                .and_then(|value| value.to_str().ok()),
            Some("17")
        );
        assert!(matches!(
            AppError::from(BackendError::Forbidden("订阅未批准".into())),
            AppError::Forbidden(_)
        ));
    }

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
    fn version_conflicts_have_409_in_both_error_paths() {
        let error = AppError::from(BackendError::Conflict("对象版本冲突".into()));
        assert_eq!(error.status_code(), StatusCode::CONFLICT);
        assert_eq!(error.error_response().status(), StatusCode::CONFLICT);
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
            _ => panic!("Expected AppError::Internal, got {app_err:?}"),
        }
    }
}
