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

use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct SaveJobReq {
    pub name: String,
    pub code: String,
    pub executor_id: u64,
    pub info: String,
    pub timeout: u64,
    pub max_retry: u8,
    pub max_parallel: u8,
    pub work_dir: String,
    pub work_user: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    pub args: Vec<JobFormalArg>,
}

#[derive(Debug, Serialize)]
pub struct JobFormalArg {
    pub name: String,
    pub val: String,
    pub info: String,
}

#[derive(Debug, Deserialize)]
pub struct SaveJobResp {
    pub result: u64,
}

#[derive(Debug, Deserialize)]
pub struct QueryJobResp {
    pub list: Vec<JobRecord>,
    pub total: u64,
}

#[derive(Debug, Deserialize)]
pub struct JobRecord {
    pub id: u64,
    pub eid: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct SaveTimerReq {
    pub name: String,
    pub eid: String,
    pub job_type: String,
    pub timer_expr: TimerExpr,
    pub job_args: Vec<JobFormalArg>,
    pub info: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
}

/// JIA's `year` field is the sixth cron field (day of week), despite its name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerExpr {
    pub timezone: String,
    pub second: String,
    pub minute: String,
    pub hour: String,
    pub day_of_month: String,
    pub month: String,
    pub year: String,
}

impl TimerExpr {
    pub fn validate(&self) -> anyhow::Result<()> {
        if [
            &self.second,
            &self.minute,
            &self.hour,
            &self.day_of_month,
            &self.month,
            &self.year,
        ]
        .iter()
        .any(|value| value.is_empty() || value.chars().any(char::is_whitespace))
        {
            anyhow::bail!("定时器的六个字段必须分别为非空单字段表达式");
        }
        self.as_custom().validate()
    }

    pub fn from_cron(cron: &str) -> anyhow::Result<Self> {
        let fields = crate::cron::fields(cron)?;
        Ok(Self {
            timezone: "utc".into(),
            second: fields[0].clone(),
            minute: fields[1].clone(),
            hour: fields[2].clone(),
            day_of_month: fields[3].clone(),
            month: fields[4].clone(),
            year: fields[5].clone(),
        })
    }

    pub fn to_cron(&self) -> String {
        if self.second == "0" {
            format!(
                "{} {} {} {} {}",
                self.minute, self.hour, self.day_of_month, self.month, self.year
            )
        } else {
            format!(
                "{} {} {} {} {} {}",
                self.second, self.minute, self.hour, self.day_of_month, self.month, self.year
            )
        }
    }

    pub fn as_custom(&self) -> CustomTimerExpr {
        CustomTimerExpr {
            timezone: self.timezone.clone(),
            expr: format!(
                "{} {} {} {} {} {}",
                self.second, self.minute, self.hour, self.day_of_month, self.month, self.year
            ),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomTimerExpr {
    pub timezone: String,
    pub expr: String,
}

impl CustomTimerExpr {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.timezone != "utc" {
            anyhow::bail!("社区调度适配目前只支持 utc；其他值会被 worker 当作本地时区");
        }
        crate::cron::fields(&self.expr)?;
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
pub struct QueryScheduleResp {
    pub total: u64,
    pub list: Vec<ScheduleRecord>,
}

#[derive(Debug, Deserialize)]
pub struct ScheduleRecord {
    pub id: u64,
    pub eid: String,
    pub name: String,
    pub job_type: String,
    pub schedule_type: String,
    pub action: String,
    pub instance_ids: Vec<String>,
    pub timer_expr: Option<CustomTimerExpr>,
}

#[derive(Debug, Serialize)]
pub struct SaveScheduleReq {
    pub id: u64,
    pub name: String,
    pub eid: String,
    pub endpoints: Vec<DispatchEndpoint>,
    pub timer_expr: CustomTimerExpr,
    pub args: Vec<JobFormalArg>,
}

#[derive(Debug, Serialize)]
pub struct ScheduleJobReq {
    pub schedule_pid: u64,
    pub action: String,
}

#[derive(Debug, Deserialize)]
pub struct QueryRunningStatusResp {
    pub total: u64,
    pub list: Vec<RunningStatusRecord>,
}

#[derive(Debug, Deserialize)]
pub struct RunningStatusRecord {
    pub eid: String,
    pub instance_id: String,
    pub is_online: bool,
    pub schedule_name: Option<String>,
    pub schedule_status: String,
    pub next_time: String,
}

#[derive(Debug, Deserialize)]
pub struct QueryInstanceResp {
    pub total: u64,
    pub list: Vec<InstanceRecord>,
}

#[derive(Debug, Deserialize)]
pub struct InstanceRecord {
    pub instance_id: String,
    pub status: i8,
}

#[derive(Debug, Deserialize)]
pub struct SaveTimerResp {
    pub result: u64,
    pub next_exec_times: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct DispatchJobReq {
    pub eid: String,
    pub action: String,
    pub schedule_type: String,
    pub endpoints: Vec<DispatchEndpoint>,
    pub schedule_name: String,
    pub is_sync: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timer_expr: Option<TimerExpr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart_interval: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct DispatchEndpoint {
    pub instance_id: String,
}

#[derive(Debug, Deserialize)]
pub struct DispatchJobResp {
    pub result: u64,
}

#[derive(Debug, Serialize)]
pub struct QueryTimerParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_type: Option<String>,
    pub page: u64,
    pub page_size: u64,
}

#[derive(Debug, Deserialize)]
pub struct QueryJobTimerResp {
    pub list: Vec<JobTimerRecord>,
    pub total: u64,
}

#[derive(Debug, Deserialize)]
pub struct JobTimerRecord {
    pub id: u64,
    pub eid: String,
    pub name: String,
    pub job_name: String,
    pub job_type: String,
    pub timer_expr: Option<TimerExpr>,
    pub job_args: Option<serde_json::Value>,
    pub info: String,
    pub created_user: String,
    pub created_time: String,
    pub updated_time: String,
}

#[derive(Debug, Deserialize)]
pub struct QueryExecHistoryResp {
    pub list: Vec<ExecRecord>,
    pub total: u64,
}

#[derive(Debug, Deserialize)]
pub struct ExecRecord {
    pub id: u64,
    pub job_name: String,
    pub schedule_id: String,
    pub exit_status: String,
    pub exit_code: i64,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub output: String,
}

#[derive(Debug, Deserialize)]
pub struct QueryScheduleHistoryResp {
    pub list: Vec<ScheduleHistoryRecord>,
    pub total: u64,
}

#[derive(Debug, Deserialize)]
pub struct ScheduleHistoryRecord {
    pub id: u64,
    pub name: String,
    pub eid: String,
    pub schedule_type: String,
    pub created_time: String,
}

#[derive(Debug, Serialize)]
pub struct DeleteTimerReq {
    pub id: u64,
}

#[derive(Debug, Deserialize)]
pub struct DeleteTimerResp {
    pub result: u64,
}

#[derive(Debug, Serialize)]
pub struct SaveExecutorReq {
    pub name: String,
    pub command: String,
    pub platform: String,
    pub info: String,
    pub read_code_from_stdin: bool,
}

#[derive(Debug, Deserialize)]
pub struct SaveExecutorResp {
    pub result: u64,
}

#[derive(Debug, Deserialize)]
pub struct QueryExecutorResp {
    pub list: Vec<ExecutorRecord>,
    pub total: u64,
}

#[derive(Debug, Deserialize)]
pub struct ExecutorRecord {
    pub id: u64,
    pub name: String,
    pub command: String,
    pub platform: String,
}

#[derive(Debug, Deserialize)]
pub struct DashboardStats {
    pub job_num: u64,
    pub running_num: u64,
    pub exec_succ_num: u64,
    pub exec_fail_num: u64,
    pub rows: Vec<serde_json::Value>,
}
