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

//! 告警通知服务 — 多渠道发送器
//!
//! 支持渠道：邮件(SMTP)、Webhook、飞书、钉钉、企业微信

use base64::{Engine as _, engine::general_purpose::STANDARD};
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::Mailbox,
    transport::smtp::{AsyncSmtpTransportBuilder, authentication::Credentials},
};
use reqwest::{Client, StatusCode, Url, redirect::Policy};
use ring::hmac;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum NotifyChannel {
    Email(EmailConfig),
    Webhook(WebhookConfig),
    Feishu(FeishuConfig),
    DingTalk(DingTalkConfig),
    WeCom(WeComConfig),
}

#[derive(Debug, Clone)]
pub struct EmailConfig {
    pub smtp_host: String,
    pub smtp_port: u16,
    pub username: String,
    pub password: String,
    pub from: String,
    pub to: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct WebhookConfig {
    pub url: String,
    pub secret: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FeishuConfig {
    pub webhook_url: String,
    pub secret: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DingTalkConfig {
    pub webhook_url: String,
    pub secret: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WeComConfig {
    pub webhook_url: String,
    pub bot_key: String,
}

#[derive(Debug, Clone)]
pub struct NotifyMessage {
    pub title: String,
    pub content: String,
    pub level: NotifyLevel,
}

#[derive(Debug, Clone)]
pub enum NotifyLevel {
    Info,
    Warn,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotifyReceipt {
    pub channel: String,
    pub http_status: u16,
    pub provider_request_id: Option<String>,
}

/// 多渠道通知发送器
pub struct NotifySender {
    client: Option<Client>,
}

impl NotifySender {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(10))
                .redirect(Policy::none())
                .build()
                .ok(),
        }
    }

    pub async fn send(
        &self,
        channel: &NotifyChannel,
        msg: &NotifyMessage,
    ) -> Result<NotifyReceipt, String> {
        match channel {
            NotifyChannel::Email(cfg) => self.send_email(cfg, msg).await,
            NotifyChannel::Webhook(cfg) => self.send_webhook(cfg, msg).await,
            NotifyChannel::Feishu(cfg) => self.send_feishu(cfg, msg).await,
            NotifyChannel::DingTalk(cfg) => self.send_dingtalk(cfg, msg).await,
            NotifyChannel::WeCom(cfg) => self.send_wecom(cfg, msg).await,
        }
    }

    async fn send_email(
        &self,
        cfg: &EmailConfig,
        msg: &NotifyMessage,
    ) -> Result<NotifyReceipt, String> {
        let from: Mailbox = cfg
            .from
            .parse()
            .map_err(|_| "邮件发件地址无效".to_string())?;
        if cfg.to.is_empty() {
            return Err("邮件收件地址不能为空".into());
        }
        let mut builder = Message::builder().from(from).subject(&msg.title);
        for address in &cfg.to {
            let recipient: Mailbox = address
                .parse()
                .map_err(|_| "邮件收件地址无效".to_string())?;
            builder = builder.to(recipient);
        }
        let message = builder
            .body(msg.content.clone())
            .map_err(|_| "邮件内容构建失败".to_string())?;
        let transport = smtp_builder(cfg)?
            .port(cfg.smtp_port)
            .credentials(Credentials::new(cfg.username.clone(), cfg.password.clone()))
            .timeout(Some(Duration::from_secs(10)))
            .build::<Tokio1Executor>();
        transport
            .send(message)
            .await
            .map_err(|_| "SMTP 服务器拒绝或无法完成投递".to_string())?;
        Ok(NotifyReceipt {
            channel: "email".into(),
            http_status: 250,
            provider_request_id: None,
        })
    }

    async fn send_webhook(
        &self,
        cfg: &WebhookConfig,
        msg: &NotifyMessage,
    ) -> Result<NotifyReceipt, String> {
        let url = endpoint(&cfg.url)?;
        let timestamp = chrono::Utc::now().timestamp().to_string();
        let body = serde_json::json!({
            "title": msg.title,
            "content": msg.content,
            "level": format!("{:?}", msg.level),
            "timestamp": timestamp,
        });
        let encoded = serde_json::to_vec(&body).map_err(|_| "Webhook 消息编码失败".to_string())?;
        let mut request = self
            .client()?
            .post(url)
            .header("X-DTS-Timestamp", &timestamp)
            .header("Content-Type", "application/json");
        if let Some(secret) = cfg.secret.as_deref() {
            request = request.header(
                "X-DTS-Signature",
                signature(secret, &[timestamp.as_bytes(), b".", &encoded].concat()),
            );
        }
        let response = request.body(encoded).send().await.map_err(transport)?;
        success_receipt("webhook", response.status(), response.headers())
    }

    async fn send_feishu(
        &self,
        cfg: &FeishuConfig,
        msg: &NotifyMessage,
    ) -> Result<NotifyReceipt, String> {
        let color = match msg.level {
            NotifyLevel::Info => "blue",
            NotifyLevel::Warn => "yellow",
            NotifyLevel::Critical => "red",
        };

        let timestamp = chrono::Utc::now().timestamp().to_string();
        let mut body = serde_json::json!({
            "msg_type": "interactive",
            "card": {
                "header": {
                    "title": {
                        "tag": "plain_text",
                        "content": msg.title
                    },
                    "template": color
                },
                "elements": [
                    {
                        "tag": "markdown",
                        "content": msg.content
                    },
                    {
                        "tag": "note",
                        "elements": [
                            {
                                "tag": "plain_text",
                                "content": format!("DTS Platform • {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S"))
                            }
                        ]
                    }
                ]
            },
            "timestamp": timestamp
        });

        if let Some(secret) = cfg.secret.as_deref() {
            let signing = format!("{timestamp}\n{secret}");
            body["sign"] = serde_json::Value::String(signature(&signing, b""));
        }

        let resp = self
            .client()?
            .post(endpoint(&cfg.webhook_url)?)
            .json(&body)
            .send()
            .await
            .map_err(transport)?;

        provider_receipt("feishu", resp, &["code"], &["msg"]).await
    }

    async fn send_dingtalk(
        &self,
        cfg: &DingTalkConfig,
        msg: &NotifyMessage,
    ) -> Result<NotifyReceipt, String> {
        let body = serde_json::json!({
            "msgtype": "markdown",
            "markdown": {
                "title": msg.title,
                "text": format!(
                    "## {}\n\n{}\n\n> DTS Platform • {}",
                    msg.title,
                    msg.content,
                    chrono::Utc::now().format("%Y-%m-%d %H:%M:%S")
                )
            }
        });

        let mut url = endpoint(&cfg.webhook_url)?;
        if let Some(secret) = cfg.secret.as_deref() {
            let timestamp = chrono::Utc::now().timestamp_millis().to_string();
            let sign = signature(secret, format!("{timestamp}\n{secret}").as_bytes());
            url.query_pairs_mut()
                .append_pair("timestamp", &timestamp)
                .append_pair("sign", &sign);
        }
        let resp = self
            .client()?
            .post(url)
            .json(&body)
            .send()
            .await
            .map_err(transport)?;

        provider_receipt("dingtalk", resp, &["errcode"], &["errmsg"]).await
    }

    async fn send_wecom(
        &self,
        cfg: &WeComConfig,
        msg: &NotifyMessage,
    ) -> Result<NotifyReceipt, String> {
        let body = serde_json::json!({
            "msgtype": "markdown",
            "markdown": {
                "content": format!(
                    "## {}\n{}\n> DTS Platform • {}",
                    msg.title,
                    msg.content,
                    chrono::Utc::now().format("%Y-%m-%d %H:%M:%S")
                )
            }
        });

        let mut url = endpoint(&cfg.webhook_url)?;
        url.query_pairs_mut().append_pair("key", &cfg.bot_key);

        let resp = self
            .client()?
            .post(url)
            .json(&body)
            .send()
            .await
            .map_err(transport)?;

        provider_receipt("wechat_work", resp, &["errcode"], &["errmsg"]).await
    }

    fn client(&self) -> Result<&Client, String> {
        self.client
            .as_ref()
            .ok_or_else(|| "通知 HTTP 客户端初始化失败".into())
    }
}

fn smtp_builder(cfg: &EmailConfig) -> Result<AsyncSmtpTransportBuilder, String> {
    if cfg.smtp_host.trim().is_empty() || cfg.smtp_host.chars().any(char::is_whitespace) {
        return Err("SMTP 主机无效".into());
    }
    if cfg.smtp_port == 465 {
        AsyncSmtpTransport::<Tokio1Executor>::relay(&cfg.smtp_host)
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&cfg.smtp_host)
    }
    .map_err(|_| "SMTP TLS 配置无效".to_string())
}

fn endpoint(value: &str) -> Result<Url, String> {
    let url = Url::parse(value).map_err(|_| "通知地址格式无效".to_string())?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("通知地址只允许有效的 HTTP 或 HTTPS URL".into());
    }
    Ok(url)
}

fn signature(secret: &str, payload: &[u8]) -> String {
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret.as_bytes());
    STANDARD.encode(hmac::sign(&key, payload).as_ref())
}

fn transport(error: reqwest::Error) -> String {
    if error.is_timeout() {
        "通知请求超时".into()
    } else {
        "通知连接失败".into()
    }
}

fn success_receipt(
    channel: &str,
    status: StatusCode,
    headers: &reqwest::header::HeaderMap,
) -> Result<NotifyReceipt, String> {
    if !status.is_success() {
        return Err(format!("通知端点返回 HTTP {}", status.as_u16()));
    }
    Ok(NotifyReceipt {
        channel: channel.into(),
        http_status: status.as_u16(),
        provider_request_id: headers
            .get("x-request-id")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned),
    })
}

async fn provider_receipt(
    channel: &str,
    response: reqwest::Response,
    code_keys: &[&str],
    message_keys: &[&str],
) -> Result<NotifyReceipt, String> {
    let status = response.status();
    let headers = response.headers().clone();
    if !status.is_success() {
        return Err(format!("通知端点返回 HTTP {}", status.as_u16()));
    }
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|_| "通知服务返回了无效响应".to_string())?;
    let code = code_keys
        .iter()
        .find_map(|key| body.get(*key))
        .and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_str().and_then(|value| value.parse().ok()))
        });
    if code != Some(0) {
        let detail = message_keys
            .iter()
            .find_map(|key| body.get(*key).and_then(|value| value.as_str()))
            .unwrap_or("供应商拒绝通知");
        return Err(detail.chars().take(256).collect());
    }
    success_receipt(channel, status, &headers)
}

impl Default for NotifySender {
    fn default() -> Self {
        Self::new()
    }
}

/// 从平台通知渠道配置创建 NotifyChannel
pub fn build_channel_from_config(
    channel_type: &str,
    config: &serde_json::Value,
) -> Option<NotifyChannel> {
    match channel_type {
        "email" => Some(NotifyChannel::Email(EmailConfig {
            smtp_host: config["smtp_host"].as_str()?.to_string(),
            smtp_port: config["smtp_port"].as_u64()? as u16,
            username: config["username"].as_str()?.to_string(),
            password: config["password"].as_str()?.to_string(),
            from: config["from"].as_str()?.to_string(),
            to: config["to"]
                .as_array()?
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect(),
        })),
        "webhook" => Some(NotifyChannel::Webhook(WebhookConfig {
            url: config["url"].as_str()?.to_string(),
            secret: config["secret"].as_str().map(String::from),
        })),
        "feishu" => Some(NotifyChannel::Feishu(FeishuConfig {
            webhook_url: config["webhook_url"].as_str()?.to_string(),
            secret: config["secret"].as_str().map(String::from),
        })),
        "dingtalk" => Some(NotifyChannel::DingTalk(DingTalkConfig {
            webhook_url: config["webhook_url"].as_str()?.to_string(),
            secret: config["secret"].as_str().map(String::from),
        })),
        "wecom" => Some(NotifyChannel::WeCom(WeComConfig {
            webhook_url: config
                .get("webhook_url")
                .and_then(|v| v.as_str())
                .unwrap_or("https://qyapi.weixin.qq.com/cgi-bin/webhook/send")
                .to_string(),
            bot_key: config["bot_key"].as_str()?.to_string(),
        })),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, HttpRequest, HttpResponse, web};

    fn message() -> NotifyMessage {
        NotifyMessage {
            title: "test alert".into(),
            content: "threshold exceeded".into(),
            level: NotifyLevel::Warn,
        }
    }

    #[actix_rt::test]
    async fn webhook_uses_hmac_instead_of_exposing_the_secret() {
        let server = actix_test::start(|| {
            App::new().route(
                "/hook",
                web::post().to(|request: HttpRequest, body: web::Bytes| async move {
                    let timestamp = request
                        .headers()
                        .get("X-DTS-Timestamp")
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or_default();
                    let expected = signature(
                        "secret-value",
                        &[timestamp.as_bytes(), b".", body.as_ref()].concat(),
                    );
                    assert_eq!(
                        request
                            .headers()
                            .get("X-DTS-Signature")
                            .and_then(|value| value.to_str().ok()),
                        Some(expected.as_str())
                    );
                    assert!(
                        !request
                            .headers()
                            .iter()
                            .any(|(_, value)| { value.to_str().ok() == Some("secret-value") })
                    );
                    HttpResponse::Ok()
                        .insert_header(("x-request-id", "request-1"))
                        .finish()
                }),
            )
        });
        let receipt = NotifySender::new()
            .send(
                &NotifyChannel::Webhook(WebhookConfig {
                    url: server.url("/hook"),
                    secret: Some("secret-value".into()),
                }),
                &message(),
            )
            .await
            .expect("webhook should be accepted");
        assert_eq!(receipt.provider_request_id.as_deref(), Some("request-1"));
    }

    #[actix_rt::test]
    async fn provider_business_error_is_not_reported_as_delivered() {
        let server = actix_test::start(|| {
            App::new().route(
                "/wecom",
                web::post().to(|| async {
                    HttpResponse::Ok().json(serde_json::json!({
                        "errcode": 40013,
                        "errmsg": "invalid bot key"
                    }))
                }),
            )
        });
        let result = NotifySender::new()
            .send(
                &NotifyChannel::WeCom(WeComConfig {
                    webhook_url: server.url("/wecom"),
                    bot_key: "bad-key".into(),
                }),
                &message(),
            )
            .await;
        assert_eq!(
            result.expect_err("provider rejected request"),
            "invalid bot key"
        );
    }

    #[actix_rt::test]
    async fn email_rejects_invalid_recipient_before_network() {
        let result = NotifySender::new()
            .send(
                &NotifyChannel::Email(EmailConfig {
                    smtp_host: "smtp.example.com".into(),
                    smtp_port: 465,
                    username: "user".into(),
                    password: "secret".into(),
                    from: "from@example.com".into(),
                    to: vec!["invalid address".into()],
                }),
                &message(),
            )
            .await;
        assert!(result.is_err());
    }

    #[test]
    fn smtp_requires_a_valid_host_and_encrypted_transport() {
        let config = EmailConfig {
            smtp_host: "bad host".into(),
            smtp_port: 587,
            username: "user".into(),
            password: "secret".into(),
            from: "from@example.com".into(),
            to: vec!["to@example.com".into()],
        };
        assert!(smtp_builder(&config).is_err());
    }

    #[test]
    fn endpoint_rejects_non_http_schemes() {
        assert!(endpoint("file:///tmp/secret").is_err());
        assert!(endpoint("not a url").is_err());
    }
}
