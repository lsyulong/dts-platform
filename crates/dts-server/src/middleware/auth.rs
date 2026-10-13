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

use crate::models::{auth::AuthIdentity, error::AppError};
use crate::services::auth_svc::AuthService;
use actix_web::{HttpRequest, web};

pub fn bearer_token(req: &HttpRequest) -> Result<&str, AppError> {
    req.headers()
        .get("Authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.is_empty() && !value.chars().any(char::is_whitespace))
        .ok_or_else(|| AppError::Unauthorized("缺少有效的 Bearer Token".into()))
}

pub async fn authenticate_request(req: &HttpRequest) -> Result<AuthIdentity, AppError> {
    let token = bearer_token(req)?;
    let service = req
        .app_data::<web::Data<AuthService>>()
        .ok_or_else(|| AppError::Internal("认证服务未配置".into()))?;
    service.authenticate(token).await
}

pub async fn require_permission(
    req: &HttpRequest,
    permission: &str,
) -> Result<AuthIdentity, AppError> {
    let identity = authenticate_request(req).await?;
    let service = req
        .app_data::<web::Data<AuthService>>()
        .ok_or_else(|| AppError::Internal("认证服务未配置".into()))?;
    service.authorize(&identity, permission).await?;
    Ok(identity)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bearer_transport_accepts_an_opaque_token_and_rejects_missing_or_malformed_values() {
        let req = actix_web::test::TestRequest::default()
            .insert_header(("Authorization", "Bearer opaque_session_token_123"))
            .to_http_request();
        assert_eq!(bearer_token(&req).unwrap(), "opaque_session_token_123");
        for value in [
            "",
            "Bearer ",
            "Basic arbitrary",
            "Bearer two tokens",
            "arbitrary",
        ] {
            let req = actix_web::test::TestRequest::default()
                .insert_header(("Authorization", value))
                .to_http_request();
            assert!(matches!(bearer_token(&req), Err(AppError::Unauthorized(_))));
        }
        let req = actix_web::test::TestRequest::default().to_http_request();
        assert!(matches!(bearer_token(&req), Err(AppError::Unauthorized(_))));
    }
}
