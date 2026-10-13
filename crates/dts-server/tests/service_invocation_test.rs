mod common;
#[path = "product_job_service_support/masking.rs"]
mod masking;
mod service_invocation_support;

use actix_web::{App, http::StatusCode, test, web};
use dts_backend::repo::service_access::ServiceCallOutcome;
use dts_server::{
    api::service_invocation::{self, invoke},
    models::error::AppError,
    services::service_invocation::ServiceInvokeRequest,
};
use serde_json::{Value, json};
use service_invocation_support::{Fixture, fixture, query_audits};
use std::sync::atomic::Ordering;

struct HttpObservation {
    status: StatusCode,
    body: Value,
    retry: Option<u32>,
    cache_control: Option<String>,
}
async fn http(f: &Fixture, body: Value) -> HttpObservation {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(f.invocation.clone()))
            .service(
                web::resource("/api/v1/data-services/{api_id}/invoke")
                    .app_data(service_invocation::json_config())
                    .route(web::post().to(invoke)),
            ),
    )
    .await;
    let request = test::TestRequest::post()
        .uri(&format!("/api/v1/data-services/{}/invoke", f.api.id))
        .insert_header(("X-Application-Key", f.secret.clone()))
        .set_json(body)
        .to_request();
    let response = test::call_service(&app, request).await;
    let status = response.status();
    let retry = response
        .headers()
        .get("Retry-After")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok());
    let cache_control = response
        .headers()
        .get("Cache-Control")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let body = test::read_body_json(response).await;
    HttpObservation {
        status,
        body,
        retry,
        cache_control,
    }
}
async fn calls(f: &Fixture) -> Vec<dts_backend::repo::service_access::ServiceCallAudit> {
    f.access
        .list_calls(
            &f.actor,
            Some(f.application.id.clone()),
            Some(f.api.id.clone()),
            100,
            0,
        )
        .await
        .unwrap()
        .data
}

#[actix_web::test]
async fn external_credential_uses_actual_flight_rows_and_returns_masked_effective_schema() {
    let f = fixture(60).await;
    assert_eq!(f.observations.data_calls.load(Ordering::SeqCst), 0);
    let response = http(&f, json!({"parameters":{}})).await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    assert_eq!(response.cache_control.as_deref(), Some("no-store"));
    assert_eq!(response.body["row_count"], 2);
    assert_eq!(response.body["complete"], true);
    assert_eq!(
        response.body["source_response_schema"][1]["data_type"],
        "BIGINT"
    );
    assert_eq!(
        response.body["effective_response_schema"][1]["data_type"],
        "VARCHAR"
    );
    assert_eq!(response.body["mask_applied"], true);
    assert_eq!(response.body["rows"][0]["region"], "华东");
    let masked = response.body["rows"][0]["amount"].as_str().unwrap();
    assert!(masked.starts_with("hmac:v1:"));
    assert!(!masked.contains("123456789"));
    assert!(!response.body.to_string().contains(&f.secret));
    assert_eq!(response.body["call"]["outcome"], "succeeded");
    assert_eq!(
        response.body["call"]["external_credential_authenticated"],
        true
    );
    assert_eq!(response.body["rate_limit_enforced"], true);
    assert_eq!(f.observations.data_calls.load(Ordering::SeqCst), 1);
    let calls = calls(&f).await;
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].outcome, ServiceCallOutcome::Succeeded);
    assert_eq!(calls[0].row_count, Some(2));
    let audits = query_audits(&f).await;
    let audit = audits
        .iter()
        .find(|event| event.id == format!("service_call:{}", calls[0].id))
        .unwrap();
    assert_eq!(audit.outcome, "succeeded");
    assert_eq!(audit.evidence["row_count"], 2);
    assert_eq!(
        audit.evidence["metadata_ids"][0],
        response.body["metadata_id"]
    );
}

#[actix_web::test]
async fn revoked_credential_is_audited_and_never_dispatches_to_flight() {
    let f = fixture(60).await;
    f.access
        .revoke_credential(
            &f.actor,
            &f.application.id,
            &f.credential_id,
            1,
            "revoke-before-call",
        )
        .await
        .unwrap();
    let response = http(&f, json!({"parameters":{}})).await;
    assert_eq!(response.status, StatusCode::FORBIDDEN);
    assert!(!response.body.to_string().contains(&f.secret));
    assert_eq!(f.observations.data_calls.load(Ordering::SeqCst), 0);
    let calls = calls(&f).await;
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].outcome, ServiceCallOutcome::Denied);
    assert_eq!(
        calls[0].reason_code.as_deref(),
        Some("credential_unavailable")
    );
    assert!(calls[0].row_count.is_none() && calls[0].duration_ms.is_none());
}

#[actix_web::test]
async fn quota_denial_returns_retry_header_without_running_a_second_query() {
    let f = fixture(1).await;
    // Avoid a fixture starting its two reservations on opposite sides of a minute.
    let seconds = chrono::Utc::now().timestamp().rem_euclid(60);
    if seconds >= 57 {
        tokio::time::sleep(std::time::Duration::from_secs((61 - seconds) as u64)).await;
    }
    let first = http(&f, json!({"parameters":{}})).await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.body);
    let second = http(&f, json!({"parameters":{}})).await;
    assert_eq!(
        second.status,
        StatusCode::TOO_MANY_REQUESTS,
        "{}",
        second.body
    );
    assert!(matches!(second.retry, Some(1..=60)));
    assert_eq!(f.observations.data_calls.load(Ordering::SeqCst), 1);
    let calls = calls(&f).await;
    assert_eq!(calls.len(), 2);
    assert_eq!(
        calls
            .iter()
            .filter(|call| call.outcome == ServiceCallOutcome::Succeeded)
            .count(),
        1
    );
    let denial = calls
        .iter()
        .find(|call| call.reason_code.as_deref() == Some("rate_limited"))
        .unwrap();
    assert!(denial.row_count.is_none());
    assert_eq!(
        query_audits(&f)
            .await
            .iter()
            .filter(|event| event.outcome == "succeeded")
            .count(),
        1
    );
}

#[actix_web::test]
async fn client_execution_identity_and_sql_fields_are_rejected_before_reservation() {
    let f = fixture(60).await;
    let response=http(&f,json!({"parameters":{},"tenant_id":f.actor.tenant_id,"sql":"SELECT private","actor_id":f.actor.sub})).await;
    assert_eq!(response.status, StatusCode::BAD_REQUEST);
    assert_eq!(f.observations.data_calls.load(Ordering::SeqCst), 0);
    assert!(calls(&f).await.is_empty());
}

#[actix_web::test]
async fn credential_revoked_during_flight_query_prevents_rows_and_success_audit() {
    let f = fixture(60).await;
    f.observations.paused.store(true, Ordering::SeqCst);
    let running = f.invocation.invoke(
        &f.secret,
        &f.api.id,
        None,
        ServiceInvokeRequest {
            parameters: json!({}),
        },
    );
    let revoke = async {
        f.observations.started.notified().await;
        f.access
            .revoke_credential(
                &f.actor,
                &f.application.id,
                &f.credential_id,
                1,
                "revoke-in-flight",
            )
            .await
            .unwrap();
        f.observations.released.notify_one();
    };
    let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        tokio::join!(running, revoke)
    })
    .await
    .unwrap();
    assert!(matches!(
        result,
        Err(AppError::Forbidden(_) | AppError::Conflict(_))
    ));
    assert_eq!(f.observations.data_calls.load(Ordering::SeqCst), 1);
    let calls = calls(&f).await;
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].outcome, ServiceCallOutcome::Denied);
    assert!(calls[0].row_count.is_none());
    let audits = query_audits(&f).await;
    assert!(audits.iter().any(|event| event.outcome == "denied"));
    assert!(!audits.iter().any(|event| event.outcome == "succeeded"));
}

#[actix_web::test]
async fn privacy_policy_changed_during_flight_query_prevents_old_mask_plan_delivery() {
    let f = fixture(60).await;
    f.observations.paused.store(true, Ordering::SeqCst);
    let running = f.invocation.invoke(
        &f.secret,
        &f.api.id,
        None,
        ServiceInvokeRequest {
            parameters: json!({}),
        },
    );
    let withdraw = async {
        f.observations.started.notified().await;
        f.masking
            .withdraw(&f.actor, "masking_rule", &f.rule.id, f.rule.version)
            .await
            .unwrap();
        f.observations.released.notify_one();
    };
    let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        tokio::join!(running, withdraw)
    })
    .await
    .unwrap();
    assert!(matches!(
        result,
        Err(AppError::Forbidden(_) | AppError::Conflict(_))
    ));
    let calls = calls(&f).await;
    assert_eq!(calls[0].outcome, ServiceCallOutcome::Denied);
    assert!(calls[0].row_count.is_none());
    assert!(
        !query_audits(&f)
            .await
            .iter()
            .any(|event| event.outcome == "succeeded")
    );
}
