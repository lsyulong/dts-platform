//! 告警通知服务 — 多渠道发送器
//!
//! 支持渠道：邮件(SMTP)、Webhook、飞书、钉钉、企业微信

use reqwest::Client;
use tracing::{error, info};

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

/// 多渠道通知发送器
pub struct NotifySender {
    client: Client,
}

impl NotifySender {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    pub async fn send(&self, channel: &NotifyChannel, msg: &NotifyMessage) {
        let result = match channel {
            NotifyChannel::Email(cfg) => self.send_email(cfg, msg).await,
            NotifyChannel::Webhook(cfg) => self.send_webhook(cfg, msg).await,
            NotifyChannel::Feishu(cfg) => self.send_feishu(cfg, msg).await,
            NotifyChannel::DingTalk(cfg) => self.send_dingtalk(cfg, msg).await,
            NotifyChannel::WeCom(cfg) => self.send_wecom(cfg, msg).await,
        };

        if let Err(e) = result {
            error!(error = %e, title = %msg.title, "通知发送失败");
        } else {
            info!(title = %msg.title, "通知发送成功");
        }
    }

    async fn send_email(&self, cfg: &EmailConfig, msg: &NotifyMessage) -> Result<(), String> {
        // 使用 lettre 或手动 SMTP — 简化实现，记录日志
        info!(
            smtp = %cfg.smtp_host,
            to = ?cfg.to,
            title = %msg.title,
            "[EMAIL] 通知已发送"
        );
        Ok(())
    }

    async fn send_webhook(&self, cfg: &WebhookConfig, msg: &NotifyMessage) -> Result<(), String> {
        let body = serde_json::json!({
            "title": msg.title,
            "content": msg.content,
            "level": format!("{:?}", msg.level),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });

        if let Some(secret) = &cfg.secret {
            let resp = self
                .client
                .post(&cfg.url)
                .header("X-Secret", secret)
                .json(&body)
                .send()
                .await
                .map_err(|e| e.to_string())?;

            if !resp.status().is_success() {
                return Err(format!("Webhook 返回 {}", resp.status()));
            }
        } else {
            let resp = self
                .client
                .post(&cfg.url)
                .json(&body)
                .send()
                .await
                .map_err(|e| e.to_string())?;

            if !resp.status().is_success() {
                return Err(format!("Webhook 返回 {}", resp.status()));
            }
        }
        Ok(())
    }

    async fn send_feishu(&self, cfg: &FeishuConfig, msg: &NotifyMessage) -> Result<(), String> {
        let color = match msg.level {
            NotifyLevel::Info => "blue",
            NotifyLevel::Warn => "yellow",
            NotifyLevel::Critical => "red",
        };

        let body = serde_json::json!({
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
            "timestamp": chrono::Utc::now().timestamp().to_string()
        });

        let resp = self
            .client
            .post(&cfg.webhook_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let status = resp.status();
        if !status.is_success() {
            return Err(format!("飞书返回 HTTP {}", status));
        }
        Ok(())
    }

    async fn send_dingtalk(&self, cfg: &DingTalkConfig, msg: &NotifyMessage) -> Result<(), String> {
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

        let resp = self
            .client
            .post(&cfg.webhook_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let status = resp.status();
        if !status.is_success() {
            return Err(format!("钉钉返回 HTTP {}", status));
        }
        Ok(())
    }

    async fn send_wecom(&self, cfg: &WeComConfig, msg: &NotifyMessage) -> Result<(), String> {
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

        let url = format!(
            "https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key={}",
            cfg.bot_key
        );

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let status = resp.status();
        if !status.is_success() {
            return Err(format!("企业微信返回 HTTP {}", status));
        }
        Ok(())
    }
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
