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


//! DTS Scheduler Client — jiascheduler REST API 客户端
//!
//! 提供 DTS Platform 与 jiascheduler 之间的集成能力：
//! - 登录认证 (session cookie)
//! - 作业管理 (create/update/delete)
//! - 定时器管理 (cron trigger)
//! - 下发执行 (dispatch)
//! - 执行历史查询
//! - 调度历史查询
//! - 任务同步 (自动同步 DTS 任务到 jiascheduler)

pub mod client;
pub mod models;
pub mod sync;

pub use client::JiaSchedulerClient;
pub use sync::TaskSync;
