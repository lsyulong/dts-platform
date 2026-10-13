// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use dts_backend::repo::alert_policy::{
    AlertDeliveryLease, AlertDeliveryOutcome, AlertPolicyRepo, ClaimAlertDeliveryParams,
    ClaimScheduledAlertPolicyParams, FinishAlertDeliveryParams, FinishScheduledAlertPolicyParams,
};
use tokio::{sync::watch, task::JoinHandle};

use crate::{
    config::AlertAutomationConfig, models::error::AppError,
    services::alert_policies::AlertPolicyService,
    services::notification_connections::NotificationConnectionService,
};

pub struct AlertAutomationWorkerHandle {
    stop: watch::Sender<bool>,
    handle: Option<JoinHandle<()>>,
}

impl AlertAutomationWorkerHandle {
    pub async fn shutdown(mut self) -> Result<(), AppError> {
        let _ = self.stop.send(true);
        if let Some(handle) = self.handle.take() {
            handle
                .await
                .map_err(|_| AppError::Internal("告警自动化 worker 退出失败".into()))?;
        }
        Ok(())
    }
}

impl Drop for AlertAutomationWorkerHandle {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
        if let Some(handle) = &self.handle {
            handle.abort();
        }
    }
}

#[derive(Clone)]
pub struct AlertAutomationService {
    repo: std::sync::Arc<dyn AlertPolicyRepo>,
    policies: AlertPolicyService,
    config: AlertAutomationConfig,
    notifications: Option<NotificationConnectionService>,
}

impl AlertAutomationService {
    pub fn new(
        repo: std::sync::Arc<dyn AlertPolicyRepo>,
        policies: AlertPolicyService,
        config: AlertAutomationConfig,
    ) -> Self {
        Self {
            repo,
            policies,
            config,
            notifications: None,
        }
    }

    pub fn with_notification_connections(
        mut self,
        notifications: NotificationConnectionService,
    ) -> Self {
        self.notifications = Some(notifications);
        self
    }

    pub fn spawn_worker(&self) -> Option<AlertAutomationWorkerHandle> {
        if !self.config.enabled {
            return None;
        }
        let service = self.clone();
        let (stop, mut stopped) = watch::channel(false);
        let handle = tokio::spawn(async move {
            let worker_id = format!("alert-{}", uuid::Uuid::new_v4());
            loop {
                if *stopped.borrow() {
                    break;
                }
                if let Err(error) = service.run_once(&worker_id).await {
                    tracing::warn!(error = %error, "alert automation worker iteration failed");
                }
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(service.config.poll_seconds)) => {},
                    _ = stopped.changed() => break,
                }
            }
        });
        Some(AlertAutomationWorkerHandle {
            stop,
            handle: Some(handle),
        })
    }

    pub async fn run_once(&self, worker_id: &str) -> Result<bool, AppError> {
        self.repo.sync_schedules().await?;
        let scheduled = self.run_one_schedule(worker_id).await?;
        let delivered = self.run_one_delivery(worker_id).await?;
        Ok(scheduled || delivered)
    }

    async fn run_one_schedule(&self, worker_id: &str) -> Result<bool, AppError> {
        let Some(claim) = self
            .repo
            .claim_scheduled(&ClaimScheduledAlertPolicyParams {
                worker_id: worker_id.into(),
                lease_seconds: self.config.lease_seconds,
            })
            .await?
        else {
            return Ok(false);
        };
        let lease = AlertDeliveryLease {
            worker_id: claim.lease_owner.clone(),
            fence: claim.lease_fence,
        };
        let result = self.policies.execute_scheduled(&claim).await;
        let (delay, error) = match result {
            Ok(interval) => (interval, None),
            Err(error) => (
                i64::try_from(self.config.retry_base_seconds).unwrap_or(30),
                Some(limit_error(&error.to_string())),
            ),
        };
        self.repo
            .finish_scheduled(&FinishScheduledAlertPolicyParams {
                tenant_id: claim.tenant_id,
                policy_id: claim.policy_id,
                lease,
                next_evaluation_at: Utc::now() + ChronoDuration::seconds(delay),
                error,
            })
            .await?;
        Ok(true)
    }

    async fn run_one_delivery(&self, worker_id: &str) -> Result<bool, AppError> {
        let Some(claim) = self
            .repo
            .claim_delivery(&ClaimAlertDeliveryParams {
                worker_id: worker_id.into(),
                lease_seconds: self.config.lease_seconds,
                max_attempts: self.config.max_attempts,
            })
            .await?
        else {
            return Ok(false);
        };
        let lease = AlertDeliveryLease {
            worker_id: claim.lease_owner,
            fence: claim.lease_fence,
        };
        let result = if claim.channel == "in_app" {
            Ok(())
        } else if let Some(notifications) = &self.notifications {
            notifications
                .deliver(
                    &claim.tenant_id,
                    &claim.channel,
                    &claim.subject,
                    &claim.body,
                )
                .await
                .map(|_| ())
        } else {
            Err("通知连接服务未配置".into())
        };
        let (outcome, error, retry_at) = match result {
            Ok(()) => (AlertDeliveryOutcome::Delivered, None, None),
            Err(error) if claim.attempt_count < i64::from(self.config.max_attempts) => {
                let exponent = u32::try_from(claim.attempt_count.saturating_sub(1))
                    .unwrap_or(0)
                    .min(10);
                let delay = self
                    .config
                    .retry_base_seconds
                    .saturating_mul(2_u64.saturating_pow(exponent))
                    .min(3600);
                (
                    AlertDeliveryOutcome::Retry,
                    Some(limit_error(&error)),
                    Some(
                        Utc::now() + ChronoDuration::seconds(i64::try_from(delay).unwrap_or(3600)),
                    ),
                )
            },
            Err(error) => (
                AlertDeliveryOutcome::Failed,
                Some(limit_error(&error)),
                None,
            ),
        };
        self.repo
            .finish_delivery(&FinishAlertDeliveryParams {
                tenant_id: claim.tenant_id,
                delivery_id: claim.id,
                lease,
                outcome,
                error,
                retry_at,
            })
            .await?;
        Ok(true)
    }
}

fn limit_error(error: &str) -> String {
    error.chars().take(2048).collect()
}
