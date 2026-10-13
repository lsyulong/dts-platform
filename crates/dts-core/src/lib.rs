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

//! DTS Core — 共享基础类型
//!
//! 所有 crate 共享的领域类型定义。

pub mod types;

pub use types::*;

pub const PERMISSIONS: &[&str] = &[
    "task:read",
    "task:write",
    "task:execute",
    "datasource:read",
    "datasource:write",
    "monitor:read",
    "alert:read",
    "alert:write",
    "user:manage",
    "audit:read",
    "tenant:create",
    "assets:read",
    "assets:write",
    "assets:execute",
    "bi:read",
    "bi:write",
    "bi:execute",
    "data_security:read",
    "data_security:write",
    "data_security:execute",
    "data_security:approve",
    "data_security:all_data",
    "development:read",
    "development:write",
    "development:execute",
    "master_data:read",
    "master_data:write",
    "master_data:execute",
    "master_data:approve",
    "metadata:read",
    "metadata:write",
    "metadata:execute",
    "metrics:read",
    "metrics:write",
    "metrics:execute",
    "models:read",
    "models:write",
    "models:execute",
    "privacy:read",
    "privacy:write",
    "privacy:execute",
    "quality:read",
    "quality:write",
    "quality:execute",
    "sensitive_data:read",
    "sensitive_data:write",
    "sensitive_data:execute",
    "services:read",
    "services:write",
    "services:execute",
    "services:approve",
    "standards:read",
    "standards:write",
    "standards:execute",
    "support:read",
    "support:write",
    "support:execute",
    "system:read",
    "system:write",
    "system:execute",
    "warehouse:read",
    "warehouse:write",
    "warehouse:execute",
    "watermark:read",
    "watermark:write",
    "watermark:execute",
];
