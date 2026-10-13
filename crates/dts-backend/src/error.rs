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

/// 存储后端统一错误类型
#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("连接失败: {0}")]
    Connection(String),

    #[error("查询失败: {0}")]
    Query(String),

    #[error("记录不存在: {0}")]
    NotFound(String),

    #[error("记录已存在: {0}")]
    AlreadyExists(String),

    #[error("约束冲突: {0}")]
    ConstraintViolation(String),

    #[error("并发或关联冲突: {0}")]
    Conflict(String),

    #[error("权限拒绝: {0}")]
    Forbidden(String),

    #[error("调用次数超过每分钟上限，请在 {retry_after_secs} 秒后重试")]
    RateLimited { retry_after_secs: u32 },

    #[error("迁移失败: {0}")]
    Migration(String),

    #[error("配置错误: {0}")]
    Configuration(String),

    #[error("内部错误: {0}")]
    Internal(String),
}

impl BackendError {
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound(_))
    }

    pub fn is_already_exists(&self) -> bool {
        matches!(self, Self::AlreadyExists(_))
    }
}
