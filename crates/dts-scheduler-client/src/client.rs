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

use std::{sync::Arc, time::Duration};

use anyhow::Result;
use reqwest::{Client as HttpClient, cookie::Jar};
use serde::{Deserialize, de::DeserializeOwned};
use tracing::info;

use crate::models::*;

#[derive(Clone)]
pub struct JiaSchedulerClient {
    base_url: String,
    http_client: HttpClient,
}

#[derive(Deserialize)]
struct Envelope {
    code: i32,
    data: Option<serde_json::Value>,
}

impl JiaSchedulerClient {
    pub async fn login(base_url: &str, username: &str, password: &str) -> Result<Self> {
        let url =
            reqwest::Url::parse(base_url).map_err(|_| anyhow::anyhow!("invalid scheduler URL"))?;
        if !["http", "https"].contains(&url.scheme())
            || !url.username().is_empty()
            || url.password().is_some()
        {
            anyhow::bail!("scheduler URL must use HTTP(S) without embedded credentials");
        }
        let http_client = HttpClient::builder()
            .cookie_provider(Arc::new(Jar::default()))
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        let client = Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
            http_client,
        };
        let _: serde_json::Value = client
            .post(
                "/api/user/login",
                &serde_json::json!({
                    "username": username, "password": password,
                }),
            )
            .await?;
        info!("jiascheduler session established");
        Ok(client)
    }

    pub async fn save_job(&self, req: &SaveJobReq) -> Result<SaveJobResp> {
        self.post("/api/job/save", req).await
    }

    pub async fn query_job(
        &self,
        name: Option<&str>,
        id: Option<u64>,
        page: u64,
    ) -> Result<QueryJobResp> {
        let mut params = serde_json::json!({"job_type": "default", "page": page, "page_size": 100});
        if let Some(name) = name {
            params["name"] = name.into();
        }
        if let Some(id) = id {
            params["default_id"] = id.into();
        }
        self.get("/api/job/list", &params).await
    }

    pub async fn find_job(&self, name: &str) -> Result<Option<JobRecord>> {
        let mut found = None;
        for page in 1..=100 {
            let response = self.query_job(Some(name), None, page).await?;
            for job in response.list {
                if job.name == name {
                    if found.is_some() {
                        anyhow::bail!("duplicate scheduler job name");
                    }
                    found = Some(job);
                }
            }
            if page * 100 >= response.total {
                return Ok(found);
            }
        }
        anyhow::bail!("scheduler job lookup exceeded pagination limit")
    }

    pub async fn job_by_id(&self, id: u64) -> Result<JobRecord> {
        let mut matches = self
            .query_job(None, Some(id), 1)
            .await?
            .list
            .into_iter()
            .filter(|job| job.id == id);
        let job = matches
            .next()
            .ok_or_else(|| anyhow::anyhow!("saved scheduler job was not returned"))?;
        if matches.next().is_some() || job.eid.is_empty() {
            anyhow::bail!("saved scheduler job has an invalid identity");
        }
        Ok(job)
    }

    pub async fn save_timer(&self, req: &SaveTimerReq) -> Result<SaveTimerResp> {
        req.timer_expr.validate()?;
        self.post("/api/job/save-timer", req).await
    }

    pub async fn delete_timer(&self, id: u64) -> Result<()> {
        let response: DeleteTimerResp = self
            .post("/api/job/delete-timer", &DeleteTimerReq { id })
            .await?;
        if response.result == 0 {
            anyhow::bail!("scheduler timer was not deleted");
        }
        Ok(())
    }

    pub async fn dispatch(&self, req: &DispatchJobReq) -> Result<DispatchJobResp> {
        if let Some(timer) = &req.timer_expr {
            timer.validate()?;
        }
        self.post("/api/job/dispatch", req).await
    }

    pub async fn save_schedule(&self, req: &SaveScheduleReq) -> Result<SaveJobResp> {
        req.timer_expr.validate()?;
        self.post("/api/job/save-schedule", req).await
    }

    pub async fn schedule_action(&self, id: u64, action: &str) -> Result<DispatchJobResp> {
        self.post(
            "/api/job/schedule",
            &ScheduleJobReq {
                schedule_pid: id,
                action: action.to_owned(),
            },
        )
        .await
    }

    pub async fn find_schedule(&self, eid: &str, name: &str) -> Result<Option<ScheduleRecord>> {
        let mut found = None;
        for page in 1..=100 {
            let response: QueryScheduleResp = self.get("/api/job/schedule-list", &serde_json::json!({
                "name": name, "job_type": "default", "schedule_type": "timer", "page": page, "page_size": 100,
            })).await?;
            for schedule in response.list {
                if schedule.name == name
                    && schedule.eid == eid
                    && schedule.job_type == "default"
                    && schedule.schedule_type == "timer"
                {
                    if found.is_some() {
                        anyhow::bail!("duplicate runtime schedule for task");
                    }
                    found = Some(schedule);
                }
            }
            if page * 100 >= response.total {
                return Ok(found);
            }
        }
        anyhow::bail!("runtime schedule lookup exceeded pagination limit")
    }

    pub async fn running_status(&self, eid: &str, name: &str) -> Result<Vec<RunningStatusRecord>> {
        let mut found = Vec::new();
        for page in 1..=100 {
            let response: QueryRunningStatusResp = self.get("/api/job/running-status-list", &serde_json::json!({
                "schedule_name": name, "job_type": "default", "schedule_type": "timer", "page": page, "page_size": 100,
            })).await?;
            found.extend(
                response
                    .list
                    .into_iter()
                    .filter(|run| run.eid == eid && run.schedule_name.as_deref() == Some(name)),
            );
            if page * 100 >= response.total {
                return Ok(found);
            }
        }
        anyhow::bail!("runtime status lookup exceeded pagination limit")
    }

    pub async fn require_online_instance(&self, instance_id: &str) -> Result<()> {
        for page in 1..=100 {
            let response: QueryInstanceResp = self
                .get(
                    "/api/instance/list",
                    &serde_json::json!({
                        "status": 1, "page": page, "page_size": 100,
                    }),
                )
                .await?;
            if response
                .list
                .iter()
                .any(|instance| instance.instance_id == instance_id && instance.status == 1)
            {
                return Ok(());
            }
            if page * 100 >= response.total {
                anyhow::bail!("configured JIA_INSTANCE_ID is not online");
            }
        }
        anyhow::bail!("scheduler instance lookup exceeded pagination limit")
    }

    pub async fn query_timer(&self, params: &QueryTimerParams) -> Result<QueryJobTimerResp> {
        self.get("/api/job/timer-list", params).await
    }

    pub async fn find_timer(&self, name: &str) -> Result<Option<JobTimerRecord>> {
        let mut found = None;
        for page in 1..=100 {
            let response = self
                .query_timer(&QueryTimerParams {
                    name: Some(name.to_owned()),
                    job_type: Some("default".into()),
                    page,
                    page_size: 100,
                })
                .await?;
            for timer in response.list {
                if timer.name == name && timer.job_type == "default" {
                    if found.is_some() {
                        anyhow::bail!("duplicate scheduler timer name");
                    }
                    found = Some(timer);
                }
            }
            if page * 100 >= response.total {
                return Ok(found);
            }
        }
        anyhow::bail!("scheduler timer lookup exceeded pagination limit")
    }

    pub async fn query_exec_history(
        &self,
        eid: &str,
        page: u64,
        page_size: u64,
    ) -> Result<QueryExecHistoryResp> {
        self.get(
            "/api/job/exec-list",
            &serde_json::json!({
                "eid": eid, "job_type": "default", "page": page, "page_size": page_size,
            }),
        )
        .await
    }

    /// JIA has no EID history filter; apply an exact identity filter to its named result page.
    pub async fn query_schedule_history(
        &self,
        eid: &str,
        timer_name: &str,
    ) -> Result<QueryScheduleHistoryResp> {
        let mut response: QueryScheduleHistoryResp = self.get("/api/job/schedule-history-list", &serde_json::json!({
            "name": timer_name, "job_type": "default", "schedule_type": "timer", "page": 1, "page_size": 50,
        })).await?;
        response.list.retain(|record| {
            record.eid == eid && record.name == timer_name && record.schedule_type == "timer"
        });
        response.total = response.list.len() as u64;
        Ok(response)
    }

    pub async fn query_executor(&self) -> Result<QueryExecutorResp> {
        self.get(
            "/api/executor/list",
            &serde_json::json!({
                "name": "dts-shell", "page": 1, "page_size": 100,
            }),
        )
        .await
    }

    pub async fn save_executor(&self, name: &str, command: &str) -> Result<SaveExecutorResp> {
        self.post(
            "/api/executor/save",
            &SaveExecutorReq {
                name: name.to_owned(),
                command: command.to_owned(),
                platform: "linux".into(),
                info: "DTS Platform scheduled HTTP trigger".into(),
                read_code_from_stdin: false,
            },
        )
        .await
    }

    pub async fn get_dashboard(&self) -> Result<DashboardStats> {
        self.post(
            "/api/job/dashboard",
            &serde_json::json!({
                "job_type": "default", "filter_schedule_history": [],
            }),
        )
        .await
    }

    pub async fn ensure_dts_executor(&self) -> Result<u64> {
        let executors = self.query_executor().await?;
        if executors.total > 100 {
            anyhow::bail!("scheduler executor lookup exceeded page size");
        }
        let mut matches = executors
            .list
            .into_iter()
            .filter(|executor| executor.name == "dts-shell");
        if let Some(executor) = matches.next() {
            if matches.next().is_some()
                || executor.command != "bash -c"
                || executor.platform != "linux"
            {
                anyhow::bail!("dts-shell executor conflicts with the required Linux bash executor");
            }
            return Ok(executor.id);
        }
        Ok(self.save_executor("dts-shell", "bash -c").await?.result)
    }

    async fn post<Req: serde::Serialize, Resp: DeserializeOwned>(
        &self,
        path: &str,
        body: &Req,
    ) -> Result<Resp> {
        let response = self
            .http_client
            .post(format!("{}{path}", self.base_url))
            .json(body)
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("scheduler POST transport failure at {path}"))?;
        decode_response(response, path).await
    }

    async fn get<Params: serde::Serialize, Resp: DeserializeOwned>(
        &self,
        path: &str,
        params: &Params,
    ) -> Result<Resp> {
        let response = self
            .http_client
            .get(format!("{}{path}", self.base_url))
            .query(params)
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("scheduler GET transport failure at {path}"))?;
        decode_response(response, path).await
    }
}

async fn decode_response<T: DeserializeOwned>(
    response: reqwest::Response,
    path: &str,
) -> Result<T> {
    let status = response.status();
    if !status.is_success() {
        anyhow::bail!("scheduler HTTP failure at {path}: {status}");
    }
    let envelope: Envelope = response
        .json()
        .await
        .map_err(|_| anyhow::anyhow!("invalid scheduler response envelope at {path}"))?;
    if envelope.code != 20000 {
        anyhow::bail!("scheduler API failure at {path}: code {}", envelope.code);
    }
    let data = envelope
        .data
        .ok_or_else(|| anyhow::anyhow!("scheduler response omitted data at {path}"))?;
    serde_json::from_value(data)
        .map_err(|_| anyhow::anyhow!("invalid scheduler response data at {path}"))
}
