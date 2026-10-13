// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use chrono::{DateTime, Utc};
use dts_backend::repo::{
    CreateNotificationConnectionParams, NotificationConnection, NotificationConnectionRepo,
    RecordNotificationConnectionTestParams, SetNotificationConnectionEnabledParams,
    UpdateNotificationConnectionParams,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    models::error::AppError,
    services::notify::{
        DingTalkConfig, EmailConfig, FeishuConfig, NotifyChannel, NotifyLevel, NotifyMessage,
        NotifyReceipt, NotifySender, WeComConfig, WebhookConfig,
    },
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateNotificationConnectionRequest {
    pub name: String,
    pub channel: String,
    pub public_config: Value,
    pub secret: Option<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateNotificationConnectionRequest {
    pub expected_version: i64,
    pub name: String,
    pub public_config: Value,
    #[serde(default)]
    pub replace_secret: bool,
    pub secret: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetNotificationConnectionEnabledRequest {
    pub expected_version: i64,
    pub enabled: bool,
}

#[derive(Debug, Serialize)]
pub struct NotificationConnectionView {
    pub id: String,
    pub name: String,
    pub channel: String,
    pub public_config: Value,
    pub has_secret: bool,
    pub enabled: bool,
    pub version: i64,
    pub last_test_status: Option<String>,
    pub last_test_error: Option<String>,
    pub last_test_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct NotificationConnectionTestResult {
    pub success: bool,
    pub tested_at: DateTime<Utc>,
    pub receipt: Option<NotifyReceipt>,
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct NotificationConnectionService {
    repo: Arc<dyn NotificationConnectionRepo>,
}

impl NotificationConnectionService {
    pub fn new(repo: Arc<dyn NotificationConnectionRepo>) -> Self {
        Self { repo }
    }

    pub async fn create(
        &self,
        tenant: &str,
        actor: &str,
        request: CreateNotificationConnectionRequest,
    ) -> Result<NotificationConnectionView, AppError> {
        validate_name(&request.name)?;
        validate_config(
            &request.channel,
            &request.public_config,
            request.secret.as_deref(),
        )?;
        let value = self
            .repo
            .create(&CreateNotificationConnectionParams {
                id: Uuid::new_v4().to_string(),
                tenant_id: tenant.into(),
                name: request.name.trim().into(),
                channel: request.channel,
                public_config: request.public_config,
                secret: request.secret,
                enabled: request.enabled,
                actor_id: actor.into(),
            })
            .await?;
        Ok(view(value))
    }

    pub async fn list(&self, tenant: &str) -> Result<Vec<NotificationConnectionView>, AppError> {
        Ok(self
            .repo
            .list(tenant)
            .await?
            .into_iter()
            .map(view)
            .collect())
    }

    pub async fn get(
        &self,
        tenant: &str,
        id: &str,
    ) -> Result<NotificationConnectionView, AppError> {
        let value = self
            .repo
            .get(tenant, id)
            .await?
            .ok_or_else(|| AppError::NotFound("通知连接不存在或不属于当前租户".into()))?;
        Ok(view(value))
    }

    pub async fn update(
        &self,
        tenant: &str,
        id: &str,
        actor: &str,
        request: UpdateNotificationConnectionRequest,
    ) -> Result<NotificationConnectionView, AppError> {
        validate_name(&request.name)?;
        let existing = self
            .repo
            .get(tenant, id)
            .await?
            .ok_or_else(|| AppError::NotFound("通知连接不存在或不属于当前租户".into()))?;
        let effective_secret = if request.replace_secret {
            request.secret.as_deref()
        } else {
            existing.secret.as_deref()
        };
        validate_config(&existing.channel, &request.public_config, effective_secret)?;
        Ok(view(
            self.repo
                .update(&UpdateNotificationConnectionParams {
                    tenant_id: tenant.into(),
                    id: id.into(),
                    expected_version: request.expected_version,
                    name: request.name.trim().into(),
                    public_config: request.public_config,
                    replace_secret: request.replace_secret,
                    secret: request.secret,
                    actor_id: actor.into(),
                })
                .await?,
        ))
    }

    pub async fn set_enabled(
        &self,
        tenant: &str,
        id: &str,
        actor: &str,
        request: SetNotificationConnectionEnabledRequest,
    ) -> Result<NotificationConnectionView, AppError> {
        if request.enabled {
            let existing = self
                .repo
                .get(tenant, id)
                .await?
                .ok_or_else(|| AppError::NotFound("通知连接不存在或不属于当前租户".into()))?;
            validate_config(
                &existing.channel,
                &existing.public_config,
                existing.secret.as_deref(),
            )?;
        }
        Ok(view(
            self.repo
                .set_enabled(&SetNotificationConnectionEnabledParams {
                    tenant_id: tenant.into(),
                    id: id.into(),
                    expected_version: request.expected_version,
                    enabled: request.enabled,
                    actor_id: actor.into(),
                })
                .await?,
        ))
    }

    pub async fn delete(
        &self,
        tenant: &str,
        id: &str,
        version: i64,
        actor: &str,
    ) -> Result<(), AppError> {
        self.repo.delete(tenant, id, version, actor).await?;
        Ok(())
    }

    pub async fn test(
        &self,
        tenant: &str,
        id: &str,
        actor: &str,
    ) -> Result<NotificationConnectionTestResult, AppError> {
        let connection = self
            .repo
            .get(tenant, id)
            .await?
            .ok_or_else(|| AppError::NotFound("通知连接不存在或不属于当前租户".into()))?;
        let channel = build_channel(&connection)?;
        let tested_at = Utc::now();
        let outcome = NotifySender::new()
            .send(
                &channel,
                &NotifyMessage {
                    title: "DTS Platform 通知连接测试".into(),
                    content: "这是一条由数据中台发送的连接测试消息。".into(),
                    level: NotifyLevel::Info,
                },
            )
            .await;
        let (status, error, receipt) = match outcome {
            Ok(receipt) => ("success", None, Some(receipt)),
            Err(error) => ("failed", Some(error), None),
        };
        self.repo
            .record_test(&RecordNotificationConnectionTestParams {
                tenant_id: tenant.into(),
                id: id.into(),
                status: status.into(),
                error: error.clone(),
                actor_id: actor.into(),
                tested_at,
            })
            .await?;
        Ok(NotificationConnectionTestResult {
            success: status == "success",
            tested_at,
            receipt,
            error,
        })
    }

    /// Deliver one internal outbox message through the tenant's enabled channel.
    /// A channel must resolve to exactly one connection so routing stays deterministic.
    pub async fn deliver(
        &self,
        tenant: &str,
        channel: &str,
        title: &str,
        body: &str,
    ) -> Result<NotifyReceipt, String> {
        let matches: Vec<_> = self
            .repo
            .list(tenant)
            .await
            .map_err(|_| "读取通知连接失败".to_string())?
            .into_iter()
            .filter(|connection| connection.enabled && connection.channel == channel)
            .collect();
        let selected = match matches.as_slice() {
            [] => return Err(format!("租户没有启用的 {channel} 通知连接")),
            [selected] => selected,
            _ => {
                return Err(format!(
                    "租户存在多个启用的 {channel} 通知连接，无法确定投递目标"
                ));
            },
        };
        let connection = self
            .repo
            .get(tenant, &selected.id)
            .await
            .map_err(|_| "读取通知连接凭据失败".to_string())?
            .ok_or_else(|| "通知连接已被删除".to_string())?;
        let configured = build_channel(&connection).map_err(|error| error.to_string())?;
        NotifySender::new()
            .send(
                &configured,
                &NotifyMessage {
                    title: title.to_owned(),
                    content: body.to_owned(),
                    level: NotifyLevel::Warn,
                },
            )
            .await
    }
}

fn default_enabled() -> bool {
    true
}
fn validate_name(name: &str) -> Result<(), AppError> {
    let size = name.trim().chars().count();
    if !(1..=120).contains(&size) {
        return Err(AppError::BadRequest(
            "通知连接名称长度必须为 1 到 120 个字符".into(),
        ));
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UrlConfig {
    url: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WebhookUrlConfig {
    webhook_url: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmailPublicConfig {
    smtp_host: String,
    smtp_port: u16,
    username: String,
    from: String,
    to: Vec<String>,
}

fn parse<T: DeserializeOwned>(value: &Value) -> Result<T, AppError> {
    serde_json::from_value(value.clone())
        .map_err(|_| AppError::BadRequest("通知连接公开配置字段无效".into()))
}
fn valid_http(value: &str) -> bool {
    reqwest::Url::parse(value)
        .is_ok_and(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
}
fn require_url(value: &str) -> Result<(), AppError> {
    if !valid_http(value) {
        return Err(AppError::BadRequest(
            "通知连接地址必须是有效的 HTTP 或 HTTPS URL".into(),
        ));
    }
    Ok(())
}
fn require_secret(secret: Option<&str>) -> Result<(), AppError> {
    if secret.is_none_or(|value| value.trim().is_empty()) {
        return Err(AppError::BadRequest("该通知渠道必须配置凭据".into()));
    }
    Ok(())
}

fn validate_config(channel: &str, config: &Value, secret: Option<&str>) -> Result<(), AppError> {
    match channel {
        "webhook" => require_url(&parse::<UrlConfig>(config)?.url),
        "feishu" | "dingtalk" => require_url(&parse::<WebhookUrlConfig>(config)?.webhook_url),
        "wechat_work" => {
            require_url(&parse::<WebhookUrlConfig>(config)?.webhook_url)?;
            require_secret(secret)
        },
        "email" => {
            let value = parse::<EmailPublicConfig>(config)?;
            if value.smtp_host.trim().is_empty()
                || value.smtp_port == 0
                || value.username.trim().is_empty()
                || value.from.trim().is_empty()
                || value.to.is_empty()
                || value.to.iter().any(|item| item.trim().is_empty())
            {
                return Err(AppError::BadRequest("邮件服务器或收件人配置不完整".into()));
            }
            require_secret(secret)
        },
        _ => Err(AppError::BadRequest(
            "通知渠道仅支持 email、webhook、feishu、dingtalk、wechat_work".into(),
        )),
    }
}

fn build_channel(value: &NotificationConnection) -> Result<NotifyChannel, AppError> {
    let secret = value.secret.clone();
    match value.channel.as_str() {
        "webhook" => {
            let c = parse::<UrlConfig>(&value.public_config)?;
            Ok(NotifyChannel::Webhook(WebhookConfig { url: c.url, secret }))
        },
        "feishu" => {
            let c = parse::<WebhookUrlConfig>(&value.public_config)?;
            Ok(NotifyChannel::Feishu(FeishuConfig {
                webhook_url: c.webhook_url,
                secret,
            }))
        },
        "dingtalk" => {
            let c = parse::<WebhookUrlConfig>(&value.public_config)?;
            Ok(NotifyChannel::DingTalk(DingTalkConfig {
                webhook_url: c.webhook_url,
                secret,
            }))
        },
        "wechat_work" => {
            let c = parse::<WebhookUrlConfig>(&value.public_config)?;
            Ok(NotifyChannel::WeCom(WeComConfig {
                webhook_url: c.webhook_url,
                bot_key: secret
                    .ok_or_else(|| AppError::BadRequest("企业微信机器人密钥未配置".into()))?,
            }))
        },
        "email" => {
            let c = parse::<EmailPublicConfig>(&value.public_config)?;
            Ok(NotifyChannel::Email(EmailConfig {
                smtp_host: c.smtp_host,
                smtp_port: c.smtp_port,
                username: c.username,
                password: secret
                    .ok_or_else(|| AppError::BadRequest("邮件服务器密码未配置".into()))?,
                from: c.from,
                to: c.to,
            }))
        },
        _ => Err(AppError::BadRequest("通知渠道无效".into())),
    }
}

fn view(value: NotificationConnection) -> NotificationConnectionView {
    NotificationConnectionView {
        id: value.id,
        name: value.name,
        channel: value.channel,
        public_config: value.public_config,
        has_secret: value.has_secret,
        enabled: value.enabled,
        version: value.version,
        last_test_status: value.last_test_status,
        last_test_error: value.last_test_error,
        last_test_at: value.last_test_at,
        created_at: value.created_at,
        updated_at: value.updated_at,
    }
}
