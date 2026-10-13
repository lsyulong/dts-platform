# ape-dts、jiascheduler、SwanLake 上游能力与维护审计

审计日期：2026-10-08（Asia/Shanghai）。范围：三个本地 checkout 的 Git 状态、实际实现、公开社区代码与发布说明。此次只读检查源代码和 Git 元数据，没有 fetch、切分支、编译、启动服务或故障注入；本报告中的故障后果属于代码推断，需要后续集成试验验证。

先给判断：这三个项目分别提供数据复制、远程脚本调度、DuckDB 的 Flight SQL 接口，组合方向合理，但它们并不会天然组成一个可靠的分布式湖仓。用户已经明确，ape-dts 的 DuckDB connector 是后续计划开发并贡献社区的能力，因此本报告将其列为**有意待建能力**，不是 ape-dts 的 bug，也不是否定创业方向的理由。真正需要提前解决的是写入 ownership、CDC 的事务和恢复语义、调度故障下的重复执行、安全隔离与依赖升级。

## 1. 本地版本、社区版本与分支治理

| 项目 | 本地 HEAD / 提交日期 | 当前分支与远程 | 工作区情况 | 公开社区证据 |
|---|---|---|---|---|
| ape-dts | `e09d772a6bdb5ae081a398f0c1c8d91ec6eee8c0`，2026-05-13 | `main` → `origin/main`；origin 为 `lsyulong/ape-dts`，只有 origin | `dt-common/src/error.rs` 未暂存修改 1 行 | 社区默认 `main`；公开发布页标记 `v2.0.26` 为 Latest，页面显示 8 月 3 日；社区当前枚举已移除 Foxlake 并增加 MSSQL。社区 main SHA 本次未能可靠取得 |
| jiascheduler | `0f11762cbc3d6c5a1e27f4feafeb4124cde40fb9`，2026-07-06 | `main` → `origin/main`；origin 为 `lsyulong/jiascheduler`，只有 origin | clean | 社区默认 `main`；公开发布页标记 `v2.0.1` 为 Latest，页面显示 6 月 21 日，并有较新的 ARM 测试发布。社区 main SHA 本次未能可靠取得 |
| SwanLake | `2d14b82940be5d33f18c11830da33d851aced058`，2026-06-22 | `main` → `origin/main`；origin 为 `lsyulong/swanlake`，只有 origin | `config.rs`、`service/mod.rs`、server `main.rs` 有暂存修改，共 357 行增加、118 行删除 | 只读 `git ls-remote` 查得社区 main 为 `0f04b10e2ce5c567d47eb7591a47fcd2591afcf4`；已用固定 SHA 浏览关键源码。公开 release 的 `v0.1.0` 说明是 Flight SQL 客户端发布，不能当成成熟服务端版本证明 |

证据：[ape-dts 社区仓库](https://github.com/apecloud/ape-dts)、[ape-dts v2.0.26](https://github.com/apecloud/ape-dts/releases/tag/v2.0.26)、[jiascheduler releases](https://github.com/jiawesoft/jiascheduler/releases)、[SwanLake 社区 main 固定提交](https://github.com/swanlake-io/swanlake/commit/0f04b10e2ce5c567d47eb7591a47fcd2591afcf4)、[SwanLake releases](https://github.com/swanlake-io/swanlake/releases)。网页可能经过缓存；发布页省略年份的日期只作为页面展示，不凭它推算精确发布时间。

本地三个 HEAD 与**本地缓存的** origin/main 均为 0 ahead / 0 behind；这只证明与本地缓存 fork ref 一致，不证明与社区当前 main 一致。GitHub API 本次受到匿名 rate limit，ape 与 Jia 的 ls-remote 多次遇到连接超时，故不虚报它们的社区最新 SHA 或落后提交数。SwanLake 社区 SHA 不在本地对象库，也无法在不 fetch 的条件下计算准确分叉距离。

用户提出的“主分支同步社区，dev 做改造”原则值得采用，但实际主分支名称都是 `main`，没有 `master`，当前也没有 dev 分支。SwanLake main 的历史已有本地功能/可靠性改动和 `Merge branch 'swanlake-io:main' into main`，再叠加未提交改动，尚未实现严格的干净跟踪主分支。

建议的治理：origin 指向自己的 fork、upstream 指向社区；main 仅同步 upstream/main；dev 承载长期集成；每个贡献从对应上游基线开独立 `feature/...` / `fix/...`；平台使用审核过的 tag/SHA 与已验证二进制，不跟随浮动 main 自动部署。先保存现有 staged/unstaged 补丁和历史，再整理分支，不应直接 reset main。每次升级记录 source/sink、SQL、Arrow、DuckDB/扩展和调度接口兼容矩阵。

## 2. ape-dts：数据复制底座可用，湖仓写入是计划中的关键贡献

以下本地实现证据基于 `e09d772a`，除明确指出的 `error.rs` 外所引源码均来自该 checkout 的已提交文件。

**已观察到的能力**：MySQL / PostgreSQL 的全量与 CDC，MongoDB / Redis 等连接器、数据库/表/列过滤路由、Lua 转换、校验订正、不同数据源的并行策略、断点记录。仓库 README 的成熟路径矩阵与实际 connector/config 枚举相符。入口是 `dt-main <task_config.ini>` 独立进程，而非通用分布式查询服务。证据：[README_ZH.md](D:/rustpro/ape-dts/README_ZH.md:11)、[dt-main/src/main.rs](D:/rustpro/ape-dts/dt-main/src/main.rs:24)、[Sinker trait](D:/rustpro/ape-dts/dt-connector/src/lib.rs:20)。

**待建能力**：当前本地 DbType 枚举包含 mysql、pg、kafka、mongo、redis、clickhouse、starrocks、doris、foxlake、tidb，没有 DuckDB / DuckLake。全仓排除 `.git`、`target` 搜索也未发现 DuckDB / DuckLake / Parquet connector 和依赖。现有 Foxlake 走它自身的 S3/ORC 实现，不能作为 DuckLake 的替代。证据：[DbType](D:/rustpro/ape-dts/dt-common/src/config/config_enums.rs:17)、[sinker_config](D:/rustpro/ape-dts/dt-common/src/config/sinker_config.rs:12)、[Cargo.toml](D:/rustpro/ape-dts/Cargo.toml:23)、[Foxlake 配置](D:/rustpro/ape-dts/dt-common/src/config/task_config.rs:944)。社区当前 [config 枚举](https://github.com/apecloud/ape-dts/blob/main/dt-common/src/config/config_enums.rs) 也未声明 DuckDB；这一能力仍需要自己交付。

配置协议必须以 parser 为准：`extract_type=snapshot`、`sink_type=write` 都是小写 strum 字面值；`parallel_type` 支持 serial、snapshot、rdb_partition、rdb_merge、table 等，没有 `Snapshot_Direct_Write`。源/目标 URL、username/password、TLS 配置分别从 `[extractor]` / `[sinker]` 读取；没有通过重复 `[basic]` 参数替代这些字段的机制。INI 值由 `FromStr` 直接解析，失败处存在 unwrap/panic。平台 adapter 的协议契约测试应直接加载生成的 INI，不应仅断言生成字符串。证据：[config_enums.rs](D:/rustpro/ape-dts/dt-common/src/config/config_enums.rs:42)、[load_extractor_config](D:/rustpro/ape-dts/dt-common/src/config/task_config.rs:450)、[load_sinker_config](D:/rustpro/ape-dts/dt-common/src/config/task_config.rs:760)、[connection_auth_config](D:/rustpro/ape-dts/dt-common/src/config/connection_auth_config.rs:31)、[ini_loader](D:/rustpro/ape-dts/dt-common/src/config/ini_loader.rs:27)。

已有 checkpoint 不等于新增 sink 自动获得 exactly-once。`base_pipeline` 在间隔后记录已提交位点，recorder 写入失败只记录日志继续；现有“全量后增量不丢数据”的教程要求在全量前记录 binlog/LSN、全量后从该位点回放。新 sink 要承接至少一次回放、delete、更新主键、无主键表、事务内顺序、schema evolution 和重复事件，不能只做 append。证据：[checkpoint](D:/rustpro/ape-dts/dt-pipeline/src/base_pipeline.rs:405)、[snapshot + CDC 教程](D:/rustpro/ape-dts/docs/en/tutorial/snapshot_and_cdc_without_data_loss.md:17)。

**升级时的已确认破坏性变化**：社区 `v2.0.26` 删除 HTTP client/consumer 和停止维护的 Foxlake integrations（#527），并整理 checker 配置等。这不代表不能建立新的 sink；它说明不能把旧 HTTP pipeline 当永久的平台集成契约。新 connector 应先对齐上游愿意维护的 `Sinker` seam，并与维护者讨论目标、依赖和验收范围。[官方发布说明](https://github.com/apecloud/ape-dts/releases/tag/v2.0.26)

建议拆为 4 组贡献，避免一次提交整个平台：

1. 类型转换与目标建表：DECIMAL 精度、时间/时区、二进制、JSON、unsigned、NULL、源名称转义；附 MySQL / PG → DuckDB 的真实数据类型集成样例。
2. 批写与生命周期：本地 native DuckDB 的单 owner、连接共享规则、Appender / Arrow 批大小、内存背压、失败关闭；首先交付全量 append，报告限制。
3. CDC 应用与 checkpoint：单次事务内应用批次，目标 commit 成功后推进位点；建立可重放的事件 identity、幂等更新/删除和断电恢复测试；随后补 DDL 和特殊键。
4. DuckLake sink：PostgreSQL catalog + 本地/S3 数据路径、冲突重试、数据文件与 catalog 一致发布、维护/小文件指标；与 native 路径分开验收。

ownership 决策不可省略。本地 `.duckdb` native 文件建议由一个数据服务进程长期持有并提供批写/查询接口，多个任务向 owner 提交批次；若采用独立 CLI 直接写 native 文件，应明确独占窗口、查询服务关闭文件的时序，不能让每个 worker、每个 Flight session 都自行长期持有相同文件。DuckDB 进程内可使用多个线程和连接，因此“单 owner”不意味着“单线程”。多客户端湖仓优先 DuckLake + PostgreSQL catalog，查询服务和 ingestion 进程可以在事务协调规则下共存。[DuckDB 并发文档](https://duckdb.org/docs/current/connect/concurrency)、[DuckLake catalog 选择](https://ducklake.select/docs/stable/duckdb/usage/choosing_a_catalog_database)

平台负责连接资产、任务定义、租户权限、部署、run 生命周期和结果可视化；ape-dts 负责抽取、变换、sink、源位点和恢复。不宜把平台租户/商业授权逻辑塞入通用 connector。native owner 若新增网络接口，应单独讨论上游是否愿意承担服务化模块；可先复用标准 Flight SQL seam 或作为平台的 ingestion 服务，避免把已经删除的旧 HTTP 模式重新变成强依赖。

## 3. jiascheduler：已有多节点执行与 workflow，HA 仍需工程加固

以下本地证据基于 `0f11762c`，工作区 clean。

**已观察到**：console、comet、agent 和合并 binary；多个网络的 agent 执行脚本、定时任务、执行超时、终止、重试配置、并行限制、输出日志；workflow 及版本/发布/流程/定时接口已经存在，不应把 DAG 归类为完全没有的能力。平台可复用它分布式执行和定时底座。证据：[README.zh-CN](D:/rustpro/jiascheduler/README.zh-CN.md:50)、[workflow API](D:/rustpro/jiascheduler/openapi/src/api/workflow.rs:27)、[executor](D:/rustpro/jiascheduler/automate/src/scheduler/executor.rs:139)、[运行并行限制](D:/rustpro/jiascheduler/automate/src/scheduler/scheduler.rs:142)。

“单节点部署”是三个进程的合并包装，依然需要 MySQL 和 Redis；官方 standalone compose 包含这些外部服务。对中小企业，运维成本应按 portal + scheduler + MySQL + Redis + analytics + 数据/catalog 存储测算，而不是按 Rust binary 数量估计。NSQ 存在依赖声明，但本次读取的关键事件与 workflow 流程使用 Redis Streams；不应据此断言 NSQ 是该版本必需运行依赖。证据：[standalone compose](D:/rustpro/jiascheduler/docker-compose-standalone.yml:4)、[Bus](D:/rustpro/jiascheduler/automate/src/bus.rs:54)、[Cargo.toml](D:/rustpro/jiascheduler/Cargo.toml:129)。

**已经接入的领导者机制**：console 从 Redis 选 leader，leader 执行健康检查和 workflow timer。`LeaderElection` 不是未用代码，`openapi/src/job.rs` 实际调用它。证据：[leader_process](D:/rustpro/jiascheduler/openapi/src/job.rs:148)。

**代码观察及故障推断**：

| 观察 | 推断的产品后果 | 建议验收 |
|---|---|---|
| 获取租约用 SETNX，再单独 EXPIRE；续租 GET 后再 EXPIRE；非原子 | 两命令间宕机可留下永久锁；过期/暂停竞争可续租错误 owner；缺 fencing 时陈旧 leader 可能继续派发 | 原子 `SET NX PX` 与 Lua owner-check renewal；单调 epoch/fencing；暂停和网络分区试验 |
| Redis 错误分支只 log + sleep，没有把 is_leader 改为 false 或调用失去领导权回调 | 旧 console 在已失去有效租约后仍被 workflow scheduler 视为 master | 有效租约过期即停止派发；故障注入验证旧 leader 无法提交新 run |
| Bus 回调处理失败后仍然 XACK；读新消息 `>`，全仓未发现 XCLAIM / XAUTOCLAIM / XPENDING 处理 | 状态事件处理失败可被永久确认；worker 崩溃留下的 pending 事件缺少可见恢复路径 | 持久化成功后再 ACK；pending reclaim；死信；可观测重放 |
| Bus 固定 XTRIM 5000，运行并行计数/kill sender 存进程内 HashMap | 长时间积压或重启时需要独立 reconcile；仅 max_parallel 不保证数据任务全局去重 | durable run/attempt、去重键、租约、重启 reconcile 与漏/重执行试验 |

证据：[leader-election/lib.rs](D:/rustpro/jiascheduler/crates/leader-election/src/lib.rs:26)、[错误分支](D:/rustpro/jiascheduler/crates/leader-election/src/lib.rs:63)、[bus.rs](D:/rustpro/jiascheduler/automate/src/bus.rs:85)、[scheduler.rs](D:/rustpro/jiascheduler/automate/src/scheduler/scheduler.rs:142)。公开社区 [leader-election 源码](https://github.com/jiawesoft/jiascheduler/blob/main/crates/leader-election/src/lib.rs) 仍展示同样的非原子租约和错误分支。以上没有运行故障试验，因此是可重现的候选问题及风险分析，不是已证实生产事故。

安全基础比“完全没认证”强：console 使用 Redis session，具备 Casbin/团队权限；comet 使用 Bearer secret。仍有具体不足：用户密码存储用 salted MD5，SSH host key 检查返回 `Ok(true)`；官方 demo/compose 使用公开固定示例口令、明文 ws 和公开数据库端口。生产发行应生成独立凭据、TLS、验证主机身份并限制任意 shell 执行权限。证据：[auth middleware](D:/rustpro/jiascheduler/openapi/src/middleware/auth.rs:27)、[RBAC model](D:/rustpro/jiascheduler/openapi/src/lib.rs:89)、[password hash](D:/rustpro/jiascheduler/crates/service/src/logic/user.rs:30)、[SSH host key](D:/rustpro/jiascheduler/automate/src/ssh.rs:33)、[compose](D:/rustpro/jiascheduler/docker-compose.yml:5)。

优先贡献租约正确性、消息 ACK/pending 恢复、密码和 host key 验证；平台保留数据库作业的 run/attempt/位点可见性和 commit 结果，不能把远程命令返回成功当成数据同步已持久完成。

## 4. SwanLake：远程 DuckDB 服务有价值，分布式单查询未实现

本地审计基线 `2d14b829` + 指明的 staged 改动。并对固定社区 SHA `0f04b10e...` 的 factory/config/connection/server 进行公开校验。

**已观察到**：Arrow Flight SQL gRPC、查询、prepared statement、更新/DDL、Arrow Appender 批插入、session 内事务和句柄、session 数上限、idle cleanup、status/health、查询延迟和错误指标、DuckLake checkpoint 维护。这是适合作为平台 analytics adapter 的模块。证据：[prepared batch appender](D:/rustpro/swanlake/swanlake-core/src/service/handlers/prepared.rs:470)、[session registry](D:/rustpro/swanlake/swanlake-core/src/session/registry.rs:172)、[server](D:/rustpro/swanlake/swanlake-server/src/main.rs:30)。

**单查询能力的准确边界**：每个 session 创建一个新 in-memory DuckDB，再执行共用的 `DUCKLAKE_INIT_SQL`，每个请求在该 session 的本地 DuckDB 内执行；FlightInfo 仅返回一张本地 endpoint ticket。没有 query fragment planner、远程 fragment 执行、exchange/shuffle、跨节点 hash join/聚合归并实现。`DUCKDB_THREADS` 是本地线程数，checkpoint 的 PostgreSQL advisory lock 是维护互斥，都不能当成 MPP 查询证据。可以建设多节点服务、并发查询分流和共享湖仓存储，但单条查询仍在某个节点运行。证据：[engine factory](D:/rustpro/swanlake/swanlake-core/src/engine/factory.rs:71)、[statement endpoint](D:/rustpro/swanlake/swanlake-core/src/service/handlers/statement.rs:82)、[local execution](D:/rustpro/swanlake/swanlake-core/src/service/execute.rs:177)、[maintenance lock](D:/rustpro/swanlake/swanlake-core/src/maintenance/lock.rs:9)、[社区固定 SHA factory](https://github.com/swanlake-io/swanlake/blob/0f04b10e2ce5c567d47eb7591a47fcd2591afcf4/swanlake-core/src/engine/factory.rs)。

默认 session 的本地 CREATE TABLE/INSERT 是内存库状态，session 清理或进程终止便消失。持久化必须明确使用已附加的 DuckLake catalog（及 `USE` / fully-qualified 表名）或另行设计 native 持久化 owner；不是“起了 SwanLake 就有共享持久数仓”。DuckLake 官方也提醒，未指定 catalog 会意外写到临时内存库。[factory](D:/rustpro/swanlake/swanlake-core/src/engine/factory.rs:87)、[DuckLake 使用说明](https://ducklake.select/docs/stable/duckdb/introduction)

**结果流式的实际情况**：DuckDB Arrow iterator 被完整 `.collect()` 成 `Vec<RecordBatch>`，随后整批转成 `Vec<FlightData>` 再通过 iterator 发送。因此 Flight 在网络层采用 stream，但服务器没有真正实现有界、边读边发、按客户端消费速度背压的结果管线。大结果集可能先占满内存，首批结果也必须等完成 materialization。这一行为同时存在于社区固定 SHA，不能只凭 README 的 streaming 描述认定解决。证据：[collect_query_result](D:/rustpro/swanlake/swanlake-core/src/engine/connection.rs:297)、[转换](D:/rustpro/swanlake/swanlake-core/src/service/execute.rs:205)、[into_stream](D:/rustpro/swanlake/swanlake-core/src/service/mod.rs:97)、[社区固定 SHA connection](https://github.com/swanlake-io/swanlake/blob/0f04b10e2ce5c567d47eb7591a47fcd2591afcf4/swanlake-core/src/engine/connection.rs)。

会话以 TCP peer address 或 peer IP 识别，prepared statements 和事务状态放在实例内存。普通负载均衡、连接池重连和反向代理不能自动实现会话 HA；peer_ip 模式把同一出口 IP 的连接合并，还需要评估租户/session 泄漏风险。服务端 schema_for_query 会执行查询来取得 Arrow schema，再在 DoGet 执行查询；这种路径可能重复昂贵计算，对带副作用的语句也需验证。证据：[session ID](D:/rustpro/swanlake/swanlake-core/src/service/mod.rs:51)、[schema_for_query](D:/rustpro/swanlake/swanlake-core/src/engine/connection.rs:56)、[get_flight_info](D:/rustpro/swanlake/swanlake-core/src/service/handlers/statement.rs:47)。

**认证、隔离和资源**：当前 ServerConfig 与 server builder 没有客户认证/授权和 gRPC TLS 配置，任意 SQL 更新/DDL 会进入 engine；factory 默认允许自动加载扩展且 `allow_unsigned_extensions()`；默认 status 监听 `0.0.0.0`。init SQL 和整个 config 会写 log，若其中包含 catalog 凭据则有泄漏风险。未观察到 query deadline/cancel、结果上限、每租户 CPU/内存/I/O 配额的系统实现。与查询/结果物化的实现结合，单个客户端可能耗尽节点资源。证据：[config](D:/rustpro/swanlake/swanlake-core/src/config.rs:14)、[server builder](D:/rustpro/swanlake/swanlake-server/src/main.rs:125)、[factory](D:/rustpro/swanlake/swanlake-core/src/engine/factory.rs:62)、[community config](https://github.com/swanlake-io/swanlake/blob/0f04b10e2ce5c567d47eb7591a47fcd2591afcf4/swanlake-core/src/config.rs)、[community server](https://github.com/swanlake-io/swanlake/blob/0f04b10e2ce5c567d47eb7591a47fcd2591afcf4/swanlake-server/src/main.rs)。

本地 staged 的 `verbose_errors`/错误脱敏有积极作用，但它尚不是 HEAD 已提交功能，也不在本次读取的社区 config 中。它不能代替认证和数据权限。SwanLake AGENTS.md 一些说明（如 ENABLE_WRITES、pool/config CLI）与当前实际字段不一致，实施前应优先核对源码。

建议贡献次序：有界 Arrow 结果流与取消；认证/TLS/基于明确身份的 session；SQL 能力/扩展权限与日志脱敏；内存、并发、deadline/结果上限；持久化 catalog 配置验证；多节点查询分流、会话 affinity 和故障恢复。自行新增 MPP 应独立立项，不能包装成“多 worker 部署即可获得”。中小企业首版可从单机批量入仓起步，可恢复的多节点执行、共享 DuckLake 和并发查询扩容按客户需求启用。

## 5. 许可证与商业化边界

| 项目 | 已核实许可证 | 对计划的实际影响 |
|---|---|---|
| ape-dts | AGPL-3.0 | 商业销售和收费服务被允许，但分发与适用的网络交互场景有对应源码义务；新增 connector 并回馈社区符合用户计划 |
| jiascheduler | Unlicense | 允许修改、分发和商业使用；仍需核对其依赖许可证 |
| SwanLake | MIT | 允许修改、分发和商业使用，保留 copyright/license notice；仍需核对其依赖许可证 |

证据：[ape LICENSE](D:/rustpro/ape-dts/LICENSE:193)、[网络交互条款](D:/rustpro/ape-dts/LICENSE:540)、[公开 AGPL 原文](https://github.com/apecloud/ape-dts/blob/main/LICENSE)、[Jia LICENSE](https://github.com/jiawesoft/jiascheduler/blob/main/LICENSE)、[SwanLake LICENSE](https://github.com/swanlake-io/swanlake/blob/main/LICENSE)。

AGPL 不是“禁止商业化”。商业路线可以是开放 connector 与核心加固，销售部署、升级、备份恢复、运维服务和平台价值。也不能简单声称“子进程隔离就一定可以把全部平台闭源”：AGPL 文本区分独立 aggregate 与组合衍生作品，是否构成组合要看接口、代码复用、控制流与部署实质。可选择平台整体兼容开源路线，或在确定商业闭源范围前审核具体架构，必要时获得著作权人授权。这里记录原文事实与架构待决项，不对衍生作品范围作法律定论。

## 6. 维护与依赖风险，以及必须验证的交付门槛

ape 有持续发布和较完整的跨源测试文档，但社区主动收缩不维护的 integration，说明不能把全部扩展维护成本留给上游。Jia 有 workflow/多架构和定时修复，manifest package 仍 `1.1.5`，API 内部版本却 `2.0.1`，发布与兼容判断要看 SHA/接口，不能单看 cargo version。SwanLake 是小型早期社区，核心 crate 仍 `0.1.0`；已有健康、prepared 和事务测试不等于租户安全/HA 验证完成。上述事实应引导设置依赖 owner 与可替换 adapter，不凭 star 数判断可靠性。

本地依赖观察：ape 引用多个 ApeCloud 的 git fork，mysql binlog 有 tag、rust-postgres/reqwest 等 manifest 未固定 revision；Jia 采用多个 `*` 版本，docker-compose 使用 `latest`；SwanLake 使用 Arrow 58、tonic 0.14.5、duckdb-rs / libduckdb-sys 1.10503.1，而 README 徽章写 DuckDB v1.5.2，不能拿 Rust crate 编码直接推算 runtime 版本。需要构建后验证 `SELECT version()`、扩展版本、ABI 和目标平台，再冻结 lockfile/二进制摘要。[ape Cargo](D:/rustpro/ape-dts/Cargo.toml:49)、[Jia Cargo](D:/rustpro/jiascheduler/Cargo.toml:131)、[SwanLake Cargo](D:/rustpro/swanlake/Cargo.toml:28)、[SwanLake lockfile](D:/rustpro/swanlake/Cargo.lock:1115)。

建议对已启用依赖明确维护责任：ape 负责连接器和数据一致性，Jia 负责 durable run/租约和消息恢复，SwanLake 负责查询接口/性能/安全；平台负责产品租户契约和统一版本矩阵。单人创业阶段这些责任只能由同一人兼任，因此按需求启用模块并收窄支持矩阵，不能按三支独立维护团队估计工作量。Upstream main 用来知道社区演进，产品 release 用来知道客户运行的是什么。

交付门槛按向客户承诺的模式分层验收。首个单机批量试点无需同时完成 MySQL 与 PostgreSQL、CDC、DuckLake、多 worker 和分布式 HA；选择客户的一种实际业务源与一个目标存储，先交付完整可恢复的批量链路。

| 所承诺模式 | 该阶段必须验证 |
|---|---|
| 首个单机批量入仓试点 | 单业务源 → DuckDB 的批量抽取、类型转换、行数/关键指标核对；NULL、decimal、时区等实际使用类型；失败可见、重复运行结果可解释、任务重启可恢复；native 文件 ownership；用户身份和数据权限、凭据脱敏；备份恢复与升级回退。若使用远程 SQL 服务，还需验证约定结果规模下的内存/并发上限；若允许自由 SQL，还需实施文件、扩展和出站访问策略 |
| 启用 CDC 或 DuckLake | 在批量基线之上，验证全量/增量切换、insert/update/delete、键变更和已承诺的 schema evolution；sink commit 与 checkpoint 之间 kill、位点恢复、重复事件幂等；DuckLake 的 catalog/数据文件提交一致性、冲突重试及维护。按实际支持的源逐一验收，不先承诺两种源均覆盖 |
| 启用多 worker / 多查询节点 / HA | 再验证 ingestion 与查询共存；租约失效、leader 暂停、Redis 分区/重启和 agent 断线后的 run/attempt 恢复与 fencing；状态消息重放；session affinity；慢客户端、取消和压力下的资源回收。只有承诺多租户并发时，才验收对应租户间资源隔离 |

CDC、DuckLake、多节点执行、HA 与 MPP 是不同能力；启用哪项，就为哪项增加验收和维护预算。表中风险来自本次审计，具体性能阈值应由试点数据量、硬件和客户的服务承诺确定。

这些是产品化工作量而不是方向不值得做的证据。最值得先做的贡献是“真实可靠地把中小企业 MySQL/PG 数据送进 DuckDB/DuckLake，并以简单运维方式恢复”，随后补多节点任务和查询并发扩容。跨节点执行同一条复杂 SQL 的 MPP 应由客户负载和成本证据决定。
