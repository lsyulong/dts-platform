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


use async_trait::async_trait;

use crate::error::BackendError;
use crate::types::TaskRunInfo;

/// 创建任务运行记录的参数
#[derive(Debug, Clone)]
pub struct CreateTaskRunParams {
    pub id: String,
    pub task_id: String,
    pub status: String,
}

/// 更新任务运行记录的参数
#[derive(Debug, Clone)]
pub struct UpdateTaskRunParams {
    pub run_id: String,
    pub status: String,
    pub rows_synced: i64,
    pub bytes_synced: i64,
    pub total_rows: Option<i64>,
    pub total_bytes: Option<i64>,
    pub end_time: Option<String>,
    pub error_message: Option<String>,
}

/// 任务运行记录数据访问 trait
#[async_trait]
pub trait TaskRunRepo: Send + Sync {
    /// 创建运行记录
    async fn create(&self, params: &CreateTaskRunParams) -> Result<String, BackendError>;

    /// 更新运行记录
    async fn update(&self, params: &UpdateTaskRunParams) -> Result<bool, BackendError>;

    /// 获取任务的最近执行记录
    async fn find_recent(
        &self,
        task_id: &str,
        limit: i64,
    ) -> Result<Vec<TaskRunInfo>, BackendError>;
}
