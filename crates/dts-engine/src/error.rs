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

/// 引擎错误类型
#[derive(Debug, Clone, thiserror::Error)]
pub enum EngineError {
    /// 引擎内部错误
    #[error("引擎内部错误: {0}")]
    Internal(String),

    /// 任务/运行不存在
    #[error("任务不存在: {0}")]
    NotFound(String),

    /// 任务已被取消
    #[error("任务已取消: {0}")]
    Cancelled(String),

    /// 连接源数据库失败
    #[error("连接源数据库失败: {0}")]
    SourceConnection(String),

    /// 连接目标数据库失败
    #[error("连接目标数据库失败: {0}")]
    TargetConnection(String),

    /// 配置无效
    #[error("同步配置无效: {0}")]
    InvalidConfig(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_error_display_internal() {
        let err = EngineError::Internal("something broke".into());
        assert!(err.to_string().contains("引擎内部错误"));
        assert!(err.to_string().contains("something broke"));
    }

    #[test]
    fn engine_error_display_not_found() {
        let err = EngineError::NotFound("run-123".into());
        assert!(err.to_string().contains("任务不存在"));
        assert!(err.to_string().contains("run-123"));
    }

    #[test]
    fn engine_error_display_cancelled() {
        let err = EngineError::Cancelled("run-456".into());
        assert!(err.to_string().contains("任务已取消"));
    }

    #[test]
    fn engine_error_display_source_connection() {
        let err = EngineError::SourceConnection("connection refused".into());
        assert!(err.to_string().contains("连接源数据库失败"));
    }

    #[test]
    fn engine_error_display_target_connection() {
        let err = EngineError::TargetConnection("timeout".into());
        assert!(err.to_string().contains("连接目标数据库失败"));
    }

    #[test]
    fn engine_error_display_invalid_config() {
        let err = EngineError::InvalidConfig("missing table_mappings".into());
        assert!(err.to_string().contains("同步配置无效"));
    }
}
