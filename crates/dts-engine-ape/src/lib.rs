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


//! DTS Engine APE — ape-dts 引擎封装
//!
//! 实现 `dts_engine::EngineService` trait，通过 CLI 子进程模式调用 ape-dts。
//!
//! `ApeEngineService::submit` 生成 TOML 配置文件、启动 `dt-main` 子进程、
//! 逐行解析 stdout 提取进度信息，并通过 channel 实时回传。
//! `cancel` 通过 oneshot channel 通知子进程退出（kill + 清理临时文件）。
//!
//! 完整的子进程生命周期管理见 [`adapter`] 模块。

mod adapter;
mod service;

pub use service::ApeEngineService;
