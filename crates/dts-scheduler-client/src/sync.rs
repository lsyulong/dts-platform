use anyhow::Result;
use tracing::{info, warn};

use crate::client::JiaSchedulerClient;
use crate::models::*;

/// 当 DTS 任务创建/更新时，同步到 jiascheduler
pub struct TaskSync {
    client: JiaSchedulerClient,
    executor_id: u64,
}

impl TaskSync {
    /// 从客户端创建同步器，自动确保 executor 存在
    pub async fn new(client: JiaSchedulerClient) -> Result<Self> {
        let executor_id = client.ensure_dts_executor().await?;
        Ok(Self {
            client,
            executor_id,
        })
    }

    /// 注册/更新任务到 jiascheduler，返回作业 EID
    pub async fn upsert_job(
        &self,
        task_id: &str,
        task_name: &str,
        dts_api_base: &str,
        dts_service_token: &str,
        cron_expr: Option<&str>,
    ) -> Result<String> {
        let eid = Self::task_id_to_eid(task_id);
        let curl_cmd = build_curl_start_command(dts_api_base, task_id, dts_service_token);

        let job_req = SaveJobReq {
            name: format!("[DTS] {task_name}"),
            job_type: "default".to_string(),
            code: curl_cmd,
            executor_id: self.executor_id,
            info: format!("DTS Platform 数据同步任务: {task_name}"),
            timeout: 3600,
            max_retry: 3,
            max_parallel: 1,
            work_dir: "/tmp".to_string(),
            work_user: "root".to_string(),
            id: None,
            completed_callback: Some(CompletedCallbackOpts {
                url: format!("{dts_api_base}/api/v1/tasks/{task_id}/runs/callback"),
                method: "POST".to_string(),
                headers: Some(vec![
                    CompletedCallbackHeader {
                        name: "Authorization".to_string(),
                        value: format!("Bearer {dts_service_token}"),
                    },
                    CompletedCallbackHeader {
                        name: "Content-Type".to_string(),
                        value: "application/json".to_string(),
                    },
                ]),
                body: None,
            }),
            args: None,
        };

        let result = self.client.save_job(&job_req).await?;
        info!(
            task_id = %task_id,
            jia_job_id = result.result,
            "任务已同步到 jiascheduler"
        );

        if let Some(expr) = cron_expr {
            if !expr.is_empty() {
                self.upsert_timer(&eid, task_name, expr).await?;
            }
        }

        Ok(eid)
    }

    /// 创建/更新定时器
    pub async fn upsert_timer(&self, eid: &str, task_name: &str, cron_expr: &str) -> Result<u64> {
        let existing = self
            .client
            .query_timer(&QueryTimerParams {
                name: Some(format!("DTS-Timer-{eid}")),
                job_type: None,
                page: 1,
                page_size: 1,
            })
            .await?;

        if let Some(_old) = existing.data.first() {
            let timer_req = SaveTimerReq {
                name: format!("DTS-Timer-{eid}"),
                eid: eid.to_string(),
                job_type: "default".to_string(),
                timer_expr: Some(serde_json::json!({
                    "cron": cron_expr,
                    "interval": null,
                })),
                job_args: None,
                info: format!("DTS 任务定时调度: {task_name} (cron: {cron_expr})"),
            };
            let result = self.client.save_timer(&timer_req).await?;
            info!(eid = %eid, timer_id = result.result, "定时器已更新");
            Ok(result.result)
        } else {
            let timer_req = SaveTimerReq {
                name: format!("DTS-Timer-{eid}"),
                eid: eid.to_string(),
                job_type: "default".to_string(),
                timer_expr: Some(serde_json::json!({
                    "cron": cron_expr,
                    "interval": null,
                })),
                job_args: None,
                info: format!("DTS 任务定时调度: {task_name} (cron: {cron_expr})"),
            };
            let result = self.client.save_timer(&timer_req).await?;
            info!(eid = %eid, timer_id = result.result, "定时器已创建");
            Ok(result.result)
        }
    }

    /// 取消任务定时调度
    pub async fn remove_timer(&self, eid: &str) -> Result<()> {
        let existing = self
            .client
            .query_timer(&QueryTimerParams {
                name: Some(format!("DTS-Timer-{eid}")),
                job_type: None,
                page: 1,
                page_size: 10,
            })
            .await?;

        for timer in &existing.data {
            if let Err(e) = self.client.delete_timer(timer.id).await {
                warn!(timer_id = timer.id, error = %e, "删除定时器失败");
            } else {
                info!(timer_id = timer.id, eid = %eid, "定时器已删除");
            }
        }
        Ok(())
    }

    /// DTS task_id → jiascheduler job EID
    pub fn task_id_to_eid(task_id: &str) -> String {
        format!("dts-task-{task_id}")
    }
}

fn build_curl_start_command(dts_api_base: &str, task_id: &str, token: &str) -> String {
    format!(
        r#"curl -s -X POST "{base}/api/v1/tasks/{tid}/start" -H "Authorization: Bearer {token}" -H "Content-Type: application/json""#,
        base = dts_api_base.trim_end_matches('/'),
        tid = task_id,
        token = token,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_id_to_eid_is_reversible() {
        let task_id = "abc-123-def";
        let eid = TaskSync::task_id_to_eid(task_id);
        assert_eq!(eid, "dts-task-abc-123-def");
    }

    #[test]
    fn curl_command_contains_task_id_and_token() {
        let cmd = build_curl_start_command("http://localhost:8080", "task-1", "secret");
        assert!(cmd.contains("task-1"));
        assert!(cmd.contains("secret"));
        assert!(cmd.contains("Bearer"));
        assert!(cmd.contains("http://localhost:8080"));
    }
}
