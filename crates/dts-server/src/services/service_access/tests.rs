use super::*;
use crate::services::product::{ResourceAction, ResourceInput};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration, Utc};
use dts_backend_sqlite::{SqliteProductResourceRepo, SqliteServiceAccessRepo, SqliteUserRepo};
use serde_json::json;

#[test]
fn tokens_are_canonical_and_independent_of_console_tokens() {
    let id = "ab689734-e65a-4b2b-805f-4f04f7150582";
    let token = format!("dts_app_{id}.{}", URL_SAFE_NO_PAD.encode([7u8; 32]));
    assert_eq!(
        credentials::parse_token(&token).unwrap(),
        (id.into(), vec![7; 32])
    );
    for bad in [
        token.to_uppercase(),
        format!("{token}="),
        "sa-token-user-session".into(),
        format!("dts_app_{id}.{}", URL_SAFE_NO_PAD.encode([7u8; 31])),
    ] {
        assert!(credentials::parse_token(&bad).is_err());
    }
}

#[test]
fn credential_verifiers_bind_tenant_identity_and_private_key() {
    let key = CredentialKey::fixture([11; 32]);
    let secret = [13u8; 32];
    let verifier = key.verifier("tenant", "credential", &secret);
    assert!(key.verify("tenant", "credential", &secret, &verifier));
    assert!(!key.verify("other", "credential", &secret, &verifier));
    assert!(!key.verify("tenant", "other", &secret, &verifier));
    assert!(!key.verify("tenant", "credential", &[14; 32], &verifier));
    assert!(!CredentialKey::fixture([12; 32]).verify("tenant", "credential", &secret, &verifier));
    assert!(!key.verify("tenant", "credential", &secret, "not-hex"));
}

#[tokio::test]
async fn actual_sqlite_issue_replay_never_returns_secret_or_persists_it() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query("PRAGMA foreign_keys=ON")
        .execute(&pool)
        .await
        .unwrap();
    for sql in [
        include_str!("../../../../dts-backend-sqlite/migrations/001_initial_schema.sql"),
        include_str!("../../../../dts-backend-sqlite/migrations/002_product_resources.sql"),
        include_str!("../../../../dts-backend-sqlite/migrations/004_product_external_refs.sql"),
        include_str!("../../../../dts-backend-sqlite/migrations/015_service_access.sql"),
    ] {
        sqlx::raw_sql(sql).execute(&pool).await.unwrap();
    }
    sqlx::raw_sql("INSERT INTO tenant(id,name,slug) VALUES('tenant','T','t');
        INSERT INTO \"user\"(id,tenant_id,username,email,password_hash) VALUES('owner','tenant','owner','owner@test','hash');")
        .execute(&pool).await.unwrap();
    let service = ServiceAccessService::new(
        Arc::new(SqliteServiceAccessRepo::new(pool.clone())),
        Arc::new(SqliteProductResourceRepo::new(pool.clone())),
        Arc::new(SqliteUserRepo::new(pool.clone())),
        CredentialKey::fixture([23; 32]),
    );
    let actor = AuthIdentity {
        sub: "owner".into(),
        tenant_id: "tenant".into(),
        username: "owner".into(),
    };
    let app = service.save_application(&actor, ResourceInput {code:"app".into(), name:"App".into(), description:None,
        parent_id:None, properties:json!({"allowed_origins":[],"credential_expires_at":Utc::now()+Duration::days(5)})}).await.unwrap();
    let action = || ResourceAction {
        expected_version: app.version,
        payload: json!({"idempotency_key":"same-key"}),
    };
    let issued = service
        .issue_credential(&actor, &app.id, action())
        .await
        .unwrap();
    let secret = issued.secret.as_ref().unwrap();
    let (id, _) = credentials::parse_token(secret).unwrap();
    assert_eq!(id, issued.credential.id);
    let replay = service
        .issue_credential(&actor, &app.id, action())
        .await
        .unwrap();
    assert!(replay.issued && replay.replayed && replay.secret_not_retrievable);
    assert!(replay.secret.is_none());
    assert_eq!(replay.credential.id, issued.credential.id);
    let verifier: String = sqlx::query_scalar("SELECT verifier FROM service_credential")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_ne!(verifier, *secret);
    let receipt: String = sqlx::query_scalar("SELECT result FROM service_credential_receipt")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(!receipt.contains(secret));
    assert!(!receipt.contains(&verifier));
    let properties: String = sqlx::query_scalar(
        "SELECT properties FROM product_resource WHERE kind='service_application'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!properties.contains(secret));
    assert!(!properties.contains(&verifier));
}

#[test]
fn origin_and_literal_search_bounds_are_real() {
    use dts_backend::repo::service_access::validation::*;
    assert_eq!(
        canonical_origin("HTTPS://Example.com:443").unwrap(),
        "https://example.com"
    );
    for origin in [
        "https://*.example.com",
        "https://user:pass@example.com",
        "https://example.com/secret",
        "https://example.com?x=1",
        "file://local",
    ] {
        assert!(canonical_origin(origin).is_err(), "{origin}");
    }
    assert_eq!(
        search_pattern(Some("100%_\\")).unwrap(),
        Some("%100\\%\\_\\\\%".into())
    );
    assert!(search_pattern(Some(&"汉".repeat(129))).is_err());
    assert!(search_pattern(Some("x\n")).is_err());
}
