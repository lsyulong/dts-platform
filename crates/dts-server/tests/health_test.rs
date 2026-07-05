//! 健康检查 API 集成测试

mod common;

use actix_web::{test, App};
use dts_server::app;

#[actix_web::test]
async fn health_check_returns_200_with_healthy_status() {
    let pool = common::setup_test_db().await.unwrap();
    let config = common::test_config();

    let app = test::init_service(
        App::new()
            .app_data(actix_web::web::Data::new(pool))
            .route("/health", actix_web::web::get().to(app::health_check)),
    )
    .await;

    let req = test::TestRequest::get().uri("/health").to_request();
    let resp = test::call_service(&app, req).await;

    assert!(resp.status().is_success());
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn health_check_returns_correct_json_structure() {
    let pool = common::setup_test_db().await.unwrap();

    let app = test::init_service(
        App::new()
            .app_data(actix_web::web::Data::new(pool))
            .route("/health", actix_web::web::get().to(app::health_check)),
    )
    .await;

    let req = test::TestRequest::get().uri("/health").to_request();
    let resp = test::call_service(&app, req).await;
    let body: serde_json::Value = test::read_body_json(resp).await;

    assert_eq!(body["status"], "healthy");
    assert_eq!(body["service"], "dts-platform");
    assert!(body["version"].is_string());
}
