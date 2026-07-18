use std::sync::Arc;

use anyhow::{Context, Result};
use reqwest::{Client as HttpClient, cookie::Jar};
use serde::de::DeserializeOwned;
use tracing::{debug, info};

use crate::models::*;

/// jiascheduler REST API 客户端
///
/// 使用示例：
/// ```ignore
/// let client = JiaSchedulerClient::login("http://localhost:9090", "admin", "password").await?;
/// let timers = client.query_timer(QueryTimerParams { name: None, job_type: None, page: 1, page_size: 100 }).await?;
/// ```
#[derive(Clone)]
pub struct JiaSchedulerClient {
    base_url: String,
    http_client: HttpClient,
}

impl JiaSchedulerClient {
    /// 登录 jiascheduler 并创建客户端实例
    pub async fn login(base_url: &str, username: &str, password: &str) -> Result<Self> {
        // session 通过 cookie 管理
        let jar = Arc::new(Jar::default());
        let http_client = HttpClient::builder().cookie_provider(jar).build()?;

        let client = Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http_client,
        };

        // 发送登录请求，获取 session cookie
        let resp = client
            .http_client
            .post(format!("{}/api/user/login", client.base_url))
            .json(&serde_json::json!({
                "username": username,
                "password": password,
            }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("jiascheduler 登录失败 (HTTP {status}): {body}");
        }

        info!(base_url = %client.base_url, "jiascheduler 登录成功");
        Ok(client)
    }

    /// 创建或更新作业定义
    pub async fn save_job(&self, req: &SaveJobReq) -> Result<SaveJobResp> {
        self.post("/api/job/save", req).await
    }

    /// 创建或更新定时器
    pub async fn save_timer(&self, req: &SaveTimerReq) -> Result<SaveTimerResp> {
        self.post("/api/job/timer", req).await
    }

    /// 删除定时器
    pub async fn delete_timer(&self, id: u64) -> Result<()> {
        let _resp: serde_json::Value = self
            .post("/api/job/timer/delete", &DeleteTimerReq { id })
            .await?;
        Ok(())
    }

    /// 立即下发作业执行
    pub async fn dispatch(&self, req: &DispatchJobReq) -> Result<DispatchJobResp> {
        self.post("/api/job/dispatch", req).await
    }

    /// 查询定时器列表
    pub async fn query_timer(&self, params: &QueryTimerParams) -> Result<QueryJobTimerResp> {
        self.get("/api/job/timer/list", params).await
    }

    /// 查询执行历史
    pub async fn query_exec_history(
        &self,
        eid: &str,
        page: u64,
        page_size: u64,
    ) -> Result<QueryExecHistoryResp> {
        self.get(
            "/api/job/exec/query",
            &serde_json::json!({
                "eid": eid,
                "page": page,
                "page_size": page_size,
            }),
        )
        .await
    }

    /// 查询调度历史
    pub async fn query_schedule_history(&self, eid: &str) -> Result<QueryScheduleHistoryResp> {
        self.get(
            "/api/job/schedule/history",
            &serde_json::json!({
                "eid": eid,
                "page": 1,
                "page_size": 50,
            }),
        )
        .await
    }

    /// 查询执行器列表（用于获取默认 executor id）
    pub async fn query_executor(&self) -> Result<QueryExecutorResp> {
        self.get(
            "/api/executor/query",
            &serde_json::json!({
                "page": 1,
                "page_size": 10,
            }),
        )
        .await
    }

    /// 创建执行器（如果不存在）
    pub async fn save_executor(&self, name: &str, executor_type: &str) -> Result<SaveExecutorResp> {
        self.post(
            "/api/executor/save",
            &SaveExecutorReq {
                name: name.to_string(),
                executor_type: executor_type.to_string(),
                info: "DTS Platform 自动创建的 Shell 执行器".to_string(),
            },
        )
        .await
    }

    /// 获取仪表盘统计
    pub async fn get_dashboard(&self) -> Result<DashboardStats> {
        self.get("/api/job/dashboard", &serde_json::json!({})).await
    }

    /// 确保 DTS 执行器存在，返回 executor_id
    pub async fn ensure_dts_executor(&self) -> Result<u64> {
        let executors = self.query_executor().await?;
        for e in &executors.data {
            if e.name == "dts-shell" {
                debug!("DTS shell executor 已存在: id={}", e.id);
                return Ok(e.id);
            }
        }
        let resp = self.save_executor("dts-shell", "shell").await?;
        info!(executor_id = resp.result, "创建 DTS shell executor");
        Ok(resp.result)
    }

    // ── 内部方法 ──────────────────────────────────────────────────

    async fn post<Req: serde::Serialize, Resp: DeserializeOwned>(
        &self,
        path: &str,
        body: &Req,
    ) -> Result<Resp> {
        let url = format!("{}{}", self.base_url, path);
        debug!(url = %url, "POST jiascheduler");

        let resp = self
            .http_client
            .post(&url)
            .json(body)
            .send()
            .await
            .with_context(|| format!("jiascheduler POST 请求失败: {url}"))?;

        let status = resp.status();
        let text = resp.text().await?;

        if !status.is_success() {
            anyhow::bail!("jiascheduler API 错误 (HTTP {status}): {url} {text}");
        }

        serde_json::from_str(&text).with_context(|| format!("jiascheduler 响应解析失败: {text}"))
    }

    async fn get<Params: serde::Serialize, Resp: DeserializeOwned>(
        &self,
        path: &str,
        params: &Params,
    ) -> Result<Resp> {
        let url = format!("{}{}", self.base_url, path);
        debug!(url = %url, "GET jiascheduler");

        let resp = self
            .http_client
            .get(&url)
            .query(params)
            .send()
            .await
            .with_context(|| format!("jiascheduler GET 请求失败: {url}"))?;

        let status = resp.status();
        let text = resp.text().await?;

        if !status.is_success() {
            anyhow::bail!("jiascheduler API 错误 (HTTP {status}): {url} {text}");
        }

        serde_json::from_str(&text).with_context(|| format!("jiascheduler 响应解析失败: {text}"))
    }
}
