// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

mod common;

use std::net::SocketAddr;
use std::time::Duration;

use actix_web::{App, web};
use dts_backend::types::{CreateRoleParams, CreateTaskParams, CreateUserParams, UpdateUserParams};
use dts_server::api::auth;
use dts_server::ws::task_log::{WsHub, task_log_ws};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// A small RFC 6455 client keeps this transport regression on existing dependencies.
/// Only server frames are decoded; extended payloads are bounded to 16 KiB.
async fn read_frame(stream: &mut TcpStream) -> (u8, Vec<u8>) {
    let mut header = [0; 2];
    stream
        .read_exact(&mut header)
        .await
        .expect("WebSocket frame header");
    assert_eq!(header[1] & 0x80, 0, "Server frames must be unmasked");
    let length = match header[1] & 0x7f {
        126 => {
            let mut length = [0; 2];
            stream.read_exact(&mut length).await.unwrap();
            u16::from_be_bytes(length) as u64
        },
        127 => {
            let mut length = [0; 8];
            stream.read_exact(&mut length).await.unwrap();
            u64::from_be_bytes(length)
        },
        length => length as u64,
    };
    assert!(length <= 16 * 1024, "Unexpected oversized test frame");
    let mut payload = vec![0; length as usize];
    stream
        .read_exact(&mut payload)
        .await
        .expect("WebSocket frame payload");
    (header[0] & 0x0f, payload)
}

async fn connect_logs(address: SocketAddr, task_id: &str, token: &str) -> TcpStream {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut stream = TcpStream::connect(address).await.expect("Test TCP server");
        let request = format!(
            "GET /ws/tasks/{task_id} HTTP/1.1\r\nHost: {address}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Protocol: dts.logs\r\nAuthorization: Bearer {token}\r\n\r\n"
        );
        stream.write_all(request.as_bytes()).await.unwrap();
        // Read exactly the HTTP headers so the first WebSocket frame stays intact.
        let mut response = Vec::new();
        while !response.ends_with(b"\r\n\r\n") {
            assert!(response.len() < 16 * 1024, "Unexpected HTTP response size");
            response.push(stream.read_u8().await.expect("HTTP upgrade response"));
        }
        assert!(response.starts_with(b"HTTP/1.1 101 "), "Expected a successful WebSocket upgrade");
        let (opcode, payload) = read_frame(&mut stream).await;
        assert_eq!(opcode, 0x9, "The first authenticated heartbeat must send a ping");
        assert!(payload.is_empty());
        // Client pong with the mandatory masking flag and an empty payload.
        stream.write_all(&[0x8a, 0x80, 0, 0, 0, 0]).await.unwrap();
        stream
    }).await.expect("WebSocket upgrade deadline")
}

#[actix_web::test]
async fn established_log_connections_close_when_account_permissions_or_session_are_revoked() {
    let ctx = common::setup_test_backend().await.expect("SQLite backend");
    let tenant = ctx
        .tenant_repo
        .find_by_slug("default")
        .await
        .unwrap()
        .unwrap();
    let owner = ctx
        .user_repo
        .find_by_tenant_and_username(&tenant.id, "admin")
        .await
        .unwrap()
        .unwrap();
    let reader_role = ctx
        .role_repo
        .create_with_permissions(&CreateRoleParams {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id: tenant.id.clone(),
            name: "log-reader".into(),
            description: "WebSocket transport fixture".into(),
            is_system: false,
            permissions: vec!["task:read".into()],
        })
        .await
        .unwrap();
    let mut readers = Vec::new();
    for username in [
        "suspended-log-reader",
        "revoked-log-reader",
        "logged-out-reader",
    ] {
        let id = ctx
            .user_repo
            .create(&CreateUserParams {
                id: uuid::Uuid::new_v4().to_string(),
                tenant_id: tenant.id.clone(),
                username: username.into(),
                email: format!("{username}@example.invalid"),
                password_hash: "unused-by-token-authentication-fixture".into(),
                display_name: None,
            })
            .await
            .unwrap();
        ctx.user_repo.assign_role(&id, &reader_role).await.unwrap();
        readers.push((id, username));
    }
    let sources = ctx
        .datasource_repo
        .list_by_tenant(&tenant.id)
        .await
        .unwrap();
    let task_id = ctx
        .task_repo
        .create(&CreateTaskParams {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id: tenant.id.clone(),
            name: "Log access fixture".into(),
            description: None,
            source_id: sources[0].id.clone(),
            target_id: sources[1].id.clone(),
            cron_expr: None,
            config_json: "{}".into(),
            created_by: owner.id,
        })
        .await
        .unwrap();

    let hub = WsHub::new();
    let server_hub = hub.clone();
    let auth = common::test_auth_service(&ctx);
    let tasks = ctx.task_repo.clone();
    let server = actix_test::start(move || {
        App::new()
            .app_data(web::Data::new(auth.clone()))
            .app_data(web::Data::new(tasks.clone()))
            .app_data(web::Data::new(server_hub.clone()))
            .route("/ws/tasks/{id}", web::get().to(task_log_ws))
            .route("/logout", web::post().to(auth::logout))
    });
    let suspended_token = common::test_token(&ctx, &readers[0].0).await;
    let revoked_token = common::test_token(&ctx, &readers[1].0).await;
    let logged_out_token = common::test_token(&ctx, &readers[2].0).await;
    let mut suspended = connect_logs(server.addr(), &task_id, &suspended_token).await;
    let mut revoked = connect_logs(server.addr(), &task_id, &revoked_token).await;
    let mut logged_out = connect_logs(server.addr(), &task_id, &logged_out_token).await;

    hub.push_log(&task_id, "info", "authorized-before-revocation");
    for stream in [&mut suspended, &mut revoked, &mut logged_out] {
        let (opcode, payload) = tokio::time::timeout(Duration::from_secs(5), read_frame(stream))
            .await
            .expect("Authorized log delivery deadline");
        assert_eq!(opcode, 0x1);
        let message: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        assert_eq!(message["message"], "authorized-before-revocation");
    }

    ctx.user_repo
        .update(
            &readers[0].0,
            &UpdateUserParams {
                display_name: None,
                status: Some("disabled".into()),
            },
        )
        .await
        .unwrap();
    ctx.user_repo.remove_roles(&readers[1].0).await.unwrap();
    let response = server
        .post("/logout")
        .insert_header(("Authorization", format!("Bearer {logged_out_token}")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), actix_web::http::StatusCode::NO_CONTENT);

    // All sockets passed the initial heartbeat. The production 15-second
    // heartbeat must now close them, without reconnecting or a new HTTP request.
    let (suspended_close, revoked_close, logged_out_close) =
        tokio::time::timeout(Duration::from_secs(20), async {
            tokio::join!(
                read_frame(&mut suspended),
                read_frame(&mut revoked),
                read_frame(&mut logged_out)
            )
        })
        .await
        .expect("Existing connections must close on the next authorization heartbeat");
    for (opcode, payload) in [suspended_close, revoked_close, logged_out_close] {
        assert_eq!(opcode, 0x8, "Revoked log access must send a close frame");
        assert!(payload.len() >= 2);
        assert_eq!(
            u16::from_be_bytes([payload[0], payload[1]]),
            1008,
            "Revocation must close the socket with Policy Violation"
        );
    }
    server.stop().await;
}
