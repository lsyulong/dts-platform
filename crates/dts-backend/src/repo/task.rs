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
use crate::types::{CreateTaskParams, TaskDetail, TaskInfo, TaskRunInfo};

/// 同步任务数据访问 trait
#[async_trait]
pub trait TaskRepo: Send + Sync {
    /// 创建任务
    async fn create(&self, params: &CreateTaskParams) -> Result<String, BackendError>;

    /// 获取租户下的所有任务
    async fn list_by_tenant(&self, tenant_id: &str) -> Result<Vec<TaskInfo>, BackendError>;

    /// 获取任务详情（含关联数据源名称和最近执行记录）
    async fn find_detail_by_id(
        &self,
        id: &str,
        tenant_id: &str,
    ) -> Result<Option<TaskDetail>, BackendError>;

    /// 更新任务状态
    async fn update_status(
        &self,
        id: &str,
        tenant_id: &str,
        status: &str,
    ) -> Result<bool, BackendError>;

    /// 统计租户任务总数
    async fn count_by_tenant(&self, tenant_id: &str) -> Result<i64, BackendError>;

    /// 统计特定状态的任务数
    async fn count_by_status(&self, tenant_id: &str, status: &str) -> Result<i64, BackendError>;

    /// 获取最近执行记录
    async fn find_recent_runs(
        &self,
        task_id: &str,
        limit: i64,
    ) -> Result<Vec<TaskRunInfo>, BackendError>;
}
