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
//! 实现 `dts_engine::EngineService` trait，将 ape-dts 库接入 DTS Platform。
//!
//! 当前为骨架实现：`ApeEngineService` 的 `submit` 通过 tokio::spawn 异步启动
//! 同步任务，进度通过 channel 回传。实际的 ape-dts 调用标记为 TODO。

mod adapter;
mod service;

pub use service::ApeEngineService;
