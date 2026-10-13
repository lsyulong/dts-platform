use crate::{
    models::error::AppError,
    services::service_invocation::{ServiceInvocationService, ServiceInvokeRequest},
};
use actix_web::{HttpRequest, HttpResponse, http::header, web};

pub fn json_config() -> web::JsonConfig {
    web::JsonConfig::default()
        .limit(64 * 1024)
        .error_handler(|_, _| {
            AppError::BadRequest("调用参数格式无效或超过 64 KiB，不能包含执行身份或 SQL".into())
                .into()
        })
}

pub async fn invoke(
    req: HttpRequest,
    api_id: web::Path<String>,
    body: web::Json<ServiceInvokeRequest>,
    service: web::Data<ServiceInvocationService>,
) -> Result<HttpResponse, AppError> {
    let secret = credential(&req)?;
    let origin = if req.headers().get_all(header::ORIGIN).count() > 1 {
        Some("invalid-origin")
    } else {
        req.headers()
            .get(header::ORIGIN)
            .map(|value| value.to_str().unwrap_or("invalid-origin"))
    };
    let result = service
        .invoke(secret, &api_id, origin, body.into_inner())
        .await?;
    Ok(HttpResponse::Ok()
        .insert_header((header::CACHE_CONTROL, "no-store"))
        .insert_header(("X-Content-Type-Options", "nosniff"))
        .json(result))
}

fn credential(req: &HttpRequest) -> Result<&str, AppError> {
    let keys = req.headers().get_all("X-Application-Key").count();
    let bearers = req.headers().get_all(header::AUTHORIZATION).count();
    if keys + bearers != 1 {
        return Err(AppError::Forbidden("请提交一个有效的应用调用凭据".into()));
    }
    let secret = if keys == 1 {
        req.headers()
            .get("X-Application-Key")
            .and_then(|value| value.to_str().ok())
    } else {
        req.headers()
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
    };
    secret
        .filter(|value| {
            value.len() <= 256 && !value.is_empty() && !value.chars().any(char::is_whitespace)
        })
        .ok_or_else(|| AppError::Forbidden("应用调用凭据格式无效".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test::TestRequest;
    #[test]
    fn exactly_one_header_is_required_and_secret_is_never_returned_in_error() {
        let req = TestRequest::default()
            .insert_header(("X-Application-Key", "test-private-secret"))
            .insert_header((header::AUTHORIZATION, "Bearer another-secret"))
            .to_http_request();
        let message = credential(&req)
            .expect_err("ambiguous identity")
            .to_string();
        assert!(!message.contains("secret"));
        let duplicate = TestRequest::default()
            .append_header(("X-Application-Key", "one"))
            .append_header(("X-Application-Key", "two"))
            .to_http_request();
        assert!(credential(&duplicate).is_err());
        let req = TestRequest::default()
            .insert_header((header::AUTHORIZATION, "Bearer value"))
            .to_http_request();
        assert_eq!(credential(&req).expect("one bearer"), "value");
    }
}
