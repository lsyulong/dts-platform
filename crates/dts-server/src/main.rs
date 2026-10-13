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

use dts_server::config::AppConfig;
use dts_server::{app, db};
use tracing_subscriber::prelude::*;

enum StartupCommand {
    Serve,
    GrantPlatformAdmin(String),
}

fn startup_command(mut args: impl Iterator<Item = String>) -> anyhow::Result<StartupCommand> {
    let Some(flag) = args.next() else {
        return Ok(StartupCommand::Serve);
    };
    if flag != "--grant-platform-admin" {
        anyhow::bail!("用法: dts-server [--grant-platform-admin <user_id>]");
    }
    let user_id = args
        .next()
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("--grant-platform-admin 需要明确的现有 user_id"))?;
    if args.next().is_some() {
        anyhow::bail!("--grant-platform-admin 仅接受一个 user_id");
    }
    Ok(StartupCommand::GrantPlatformAdmin(user_id))
}

fn allow_log_target(metadata: &tracing::Metadata<'_>) -> bool {
    // Sa-Token 0.2.0 logs raw tokens in some cleanup error paths.
    !metadata.target().starts_with("sa_token_core")
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    let command = startup_command(std::env::args().skip(1))?;
    // 初始化 tracing 基础设施
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,dts_server=debug")),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_filter(tracing_subscriber::filter::filter_fn(allow_log_target)),
        )
        .init();

    // 桥接 log crate 的宏到 tracing（其他模块仍使用 log）
    tracing_log::LogTracer::init().ok();

    tracing::info!("=== DTS Platform 企业级数据同步管理平台 ===");
    tracing::info!("版本: {}", env!("CARGO_PKG_VERSION"));

    // 加载配置
    let config = AppConfig::from_env()?;
    tracing::info!("服务地址: {}:{}", config.server_host, config.server_port);
    tracing::info!("存储后端: {}", config.database_backend);

    if let StartupCommand::GrantPlatformAdmin(user_id) = command {
        let ctx =
            db::init_backend_for_command(&config.database_url, &config.database_backend).await?;
        let role_id = dts_server::services::platform_admin::grant(&ctx, &user_id).await?;
        tracing::info!(user_id = %user_id, role_id = %role_id, "离线平台管理员授权完成");
        return Ok(());
    }

    // 初始化存储后端（运行时分发）
    let bootstrap_password = match std::env::var("BOOTSTRAP_ADMIN_PASSWORD") {
        Ok(password) => Some(password),
        Err(std::env::VarError::NotPresent) => None,
        Err(std::env::VarError::NotUnicode(_)) => {
            anyhow::bail!("BOOTSTRAP_ADMIN_PASSWORD 必须是有效文本")
        },
    };
    let ctx = db::init_backend_with_bootstrap(
        &config.database_url,
        &config.database_backend,
        bootstrap_password.as_deref(),
    )
    .await?;
    drop(bootstrap_password);

    // 启动 Web 服务
    let server = app::create_app(ctx, config.clone()).await?;
    tracing::info!("服务已启动!");
    tracing::info!("健康检查: http://localhost:{}/health", config.server_port);
    tracing::info!("API 文档: 见 docs/api/ 目录");

    server.run().await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct LogBuffer(Arc<Mutex<Vec<u8>>>);

    impl Write for LogBuffer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .map_err(|_| std::io::Error::other("log buffer poisoned"))?
                .write(bytes)
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogBuffer {
        type Writer = Self;

        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    #[test]
    fn specific_debug_directives_cannot_expose_framework_tokens() {
        let output = Arc::new(Mutex::new(Vec::new()));
        let subscriber = tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::new(
                "trace,sa_token_core::auth_service=trace",
            ))
            .with(
                tracing_subscriber::fmt::layer()
                    .with_ansi(false)
                    .with_writer(LogBuffer(output.clone()))
                    .with_filter(tracing_subscriber::filter::filter_fn(allow_log_target)),
            );
        tracing::subscriber::with_default(subscriber, || {
            tracing::debug!(target: "sa_token_core::auth_service", token = "sensitive-session-value", "cleanup");
            tracing::info!(target: "dts_server", "platform-event");
        });
        let log = String::from_utf8(output.lock().unwrap().clone()).unwrap();
        assert!(log.contains("platform-event"));
        assert!(!log.contains("sensitive-session-value"));
        assert!(!log.contains("cleanup"));
    }
}
