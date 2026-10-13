# 上游依赖同步记录（2026-10-09）

本记录使用 GitHub 官方仓库 `main` 分支和本机 Git 对象核对，不把个人 fork 的 `origin/main` 当作社区最新版本。同步只更新源码 checkout，没有安装 Rust 或 Windows 工具链，也没有把浮动 `main` 直接写入生产依赖锁定策略。

三个官方仓库在本轮取得的 Git 引用中都没有发布 tag，因此当前没有可统一采用的“最新稳定 tag”。按照项目既定分支策略，社区 `main` 是持续同步基线；每次正式发布再把完成兼容验证的提交 SHA 写入发布清单。

| 项目 | 官方 upstream | 社区 main | 本地结果 |
|---|---|---|---|
| Ape-DTS | `https://github.com/apecloud/ape-dts.git` | `b6985c14423763052e211e8b38ef1d03f5af5e9d` | 本地 `main` 快进并与 `upstream/main` 完全一致；个人 fork `origin/main` 落后 21 个提交。同步前旧 `dt-common/src/error.rs` 的无效残留文本保存为 `codex-backup-before-upstream-sync-2026-10-09` stash，未重新应用到已被社区重构的文件。 |
| JiaScheduler | `https://github.com/jiawesoft/jiascheduler.git` | `7b232d8d3d6d46c8e517a8011e0ebf4026ac5558` | 本地 `main` 快进并与 `upstream/main` 完全一致；个人 fork `origin/main` 落后 28 个提交。 |
| SwanLake | `https://github.com/swanlake-io/swanlake.git` | `0f04b10e2ce5c567d47eb7591a47fcd2591afcf4` | 本地 `main` 指向并跟踪 `upstream/main`；`dev` 合并该社区提交后保留 fork 的 40 个集成提交和已暂存的错误脱敏改造。 |

`main` 表示可追溯的社区基线，`dev` 承载平台需要但尚未被上游接收的修改。正式构建仍应固定审核过的 tag 或 SHA；“已同步 main”只证明源码基线新鲜，不证明新版本已通过本项目的连接器兼容、数据正确性、性能和故障恢复验收。

当前平台没有把这三个仓库作为 Cargo `path` 或 `git` 依赖写入 `Cargo.toml`：Ape-DTS 通过 CLI 适配器调用，JiaScheduler 通过 REST 协议调用，SwanLake 通过 Arrow Flight SQL 边界调用。因此同步源码不会改写平台 `Cargo.lock`；生产可复现性由外部组件镜像或二进制所固定的审核 SHA 保证。

本轮没有编译三个完整上游仓库：当前机器资源有限，而且用户要求不安装新的 Windows Rust 工具链。平台侧继续通过适配器合同和离线编译检查验证自己的调用边界；真正升级生产版本前还需执行固定数据集兼容矩阵。
