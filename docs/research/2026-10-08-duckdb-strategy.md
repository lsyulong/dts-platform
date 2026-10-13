# DuckDB / DuckLake 技术边界与轻量数据平台的切入建议

调研日期：2026-10-08（Asia/Shanghai）。面向当前「一人开发、尚无客户」状态。本文只基于第一方资料核实技术与已有产品，商业结论属于建议；没有验证市场规模、客户支付意愿、获客成本或续费。

## 结论

DuckDB 是值得采用的执行引擎，但「Rust + DuckDB + 分布式」本身不足以构成创业产品。最适合当前资源的路径，是围绕一种可付费的业务问题交付本地或私有部署的小型分析系统，同时保留分布式任务和多个独立查询节点的扩展接口。是否继续扩大投入，应由实际客户和付费试点决定，不应先由 MPP 能否实现决定。

当前资料已经不能支持「DuckDB 完全没有多进程写入」的旧说法：原生文件的进程内模式仍有 owner 限制，但 Quack 提供服务器访问，DuckLake + PostgreSQL catalog 已提供多个客户端共同读写。三者需要分别设计和承诺。第一方依据：[DuckDB 并发文档](https://duckdb.org/docs/current/connect/concurrency)、[Quack 概览](https://duckdb.org/docs/current/quack/overview)、[DuckLake catalog 选择](https://ducklake.select/docs/stable/duckdb/usage/choosing_a_catalog_database)。

## 1. 版本事实与资料一致性

| 项目 | 本次核实结果 | 采用含义 |
| --- | --- | --- |
| DuckDB | 当前稳定版为 1.5.6，2026-09-28 发布；LTS 为 1.4.5 | 先锁定实际依赖版本、扩展版本和镜像，不能以 master 最新提交代替产品可复现发布 |
| DuckDB 2.0 | 仍为 2.0.0-dev；日历计划 2026-10-21，日期可延期 | 把 2.0 的计划与已有稳定能力分开，避免向客户承诺尚未发布的成熟度 |
| DuckLake | 稳定规范及扩展为 1.0，2026-04-13 发布；扩展要求 DuckDB 1.5.2+ | DuckDB 1.4 LTS 对应 DuckLake 0.3，选择 LTS 不等于同时能采用 DuckLake 1.0 |
| DuckLake 下一版本 | 规范 1.1 / 扩展 2.0 的计划日期为 2026-10-21 | 以 release calendar 为准；FAQ 中仍有预计 2026-09 的旧说明 |
| Quack | current 概览仍明确为 beta，协议、函数、配置和默认值可能变化 | 作为试验或兼容接口评估，暂不把它作为必须依赖的企业 SLA 基础 |

来源：[DuckDB 安装页](https://duckdb.org/install/)、[DuckDB 发布日历](https://duckdb.org/release_calendar)、[1.5.6 发布说明](https://duckdb.org/2026/09/28/announcing-duckdb-156)、[DuckLake 发布日历和兼容矩阵](https://ducklake.select/release_calendar)、[DuckLake 扩展介绍](https://ducklake.select/docs/stable/duckdb/introduction)、[Quack current 概览](https://duckdb.org/docs/current/quack/overview)。

资料存在一个小差异：Concurrency 写 Quack「as of 1.5.2」处于 beta，Quack 概览写文档描述随 1.5.3 提供的 beta。它不影响现在仍为 beta 的判断；实现时应对锁定版本直接做兼容测试，而非推断 beta 最早可用版号。

## 2. 「分布式」应拆成不同承诺

以下分层是本报告给项目设计提出的定义，不是厂商术语。

| 能力 | 解决的问题 | 是否让一条复杂 SQL 超过单节点计算能力 | 对当前项目的建议 |
| --- | --- | --- | --- |
| 单节点多线程 | 同一台机器利用多个 CPU 核 | 否 | 第一阶段默认，先测实际导入、更新、查询组合 |
| 分布式任务 | 不同 worker 执行不同同步任务、分区批次、SQL 模型或独立作业 | 通常否 | 借助现有调度器保留能力，但明确数据提交边界 |
| 多实例共享湖数据 | 多个 DuckDB 实例对同一 DuckLake 读写 | 否 | 在确有远程 worker / 多 writer 需求时启用 PostgreSQL catalog 与对象存储 |
| 查询副本 / 查询路由 | 多条独立查询分散到多个节点，增加总吞吐并隔离资源 | 否 | 优先于 MPP；需要权限、版本、快照、缓存和取消的明确契约 |
| client + server 混合查询 | 根据数据位置，把同一查询的部分运算放到客户端和服务器 | 某些阶段可在两端执行，但不等于通用集群 MPP | 现有 MotherDuck 已有，不能仅凭此形成差异 |
| 单 SQL 跨多 worker 的 MPP | 把单条 SQL 拆为扫描、局部聚合、shuffle、join、最终聚合 | 是，在可并行且网络/倾斜可控的工作负载上 | 一人、零客户阶段延后；必须有付费需求和基准证明收益后再做 |
| 高可用 | 节点故障后仍能恢复或继续提供服务 | 不相关 | 独立定义 RPO/RTO，调度器能重新分配任务不等于数据不丢失 |

不能把「多个 worker 都安装 DuckDB」「把 SQL 随机分发」「读取 S3」直接写成已经具备单 SQL MPP。相反，也不能把单 SQL 在单节点执行的平台描述成完全没有分布式能力。

## 3. 原生 DuckDB 文件、集中 server 与 DuckLake 多 writer

### 3.1 原生文件的进程内模式

读写模式由一个进程打开数据库；该进程中的不同连接/线程可使用 MVCC 和乐观并发。多个只读进程可以访问文件，但这种模式不同时允许其他进程写它。同一进程中的 append 不冲突，重叠 update/delete 可能触发事务冲突。官方还提醒对网络共享目录上的文件锁特别谨慎。来源：[Concurrency](https://duckdb.org/docs/current/connect/concurrency)。

**架构推论：** 如果 ape-dts 子进程、SQL 工作台进程和分析 worker 会访问同一个 `.duckdb`，必须明确谁拥有读写连接。首版可让一个受控执行服务持有文件；批处理在该服务内执行，或者输出到 staging 再由 owner 导入。租约、排队、取消、恢复和幂等提交由平台实现。让多个容器直接挂载共享 `.duckdb` 文件不是多 writer 方案。

### 3.2 Quack 远程协议

Quack 扩展把一个 DuckDB 实例变成 HTTP server，其他实例通过网络访问它。它提供远程访问和多客户端，仍然是服务端持有底层实例，并不会自动生成跨 worker 的 MPP 执行计划或数据库复制集。概览写 beta；当前官方部署示例是 EC2 上单实例 DuckDB、nginx 和 TLS。来源：[Quack 概览](https://duckdb.org/docs/current/quack/overview)、[Quack 部署](https://duckdb.org/docs/current/quack/setup/deployment)。

**采用建议：** 对首版自行封装的受控 query worker 不急于替换；保留协议 adapter 即可。待锁定版本经过客户端兼容、长结果流、重连、事务取消和认证测试后，再决定是否减少自建 gRPC/HTTP SQL 接口的维护工作。

### 3.3 DuckLake 的多 writer

DuckLake 的并发范围来自 catalog 选择：本地单 client 可用 DuckDB；同机多个客户端可用 SQLite，实际依赖锁和重试；远程多用户使用 PostgreSQL。不能只安装 `ducklake` 扩展并继续使用 DuckDB catalog，就宣称获得多 writer。MySQL catalog 当前官方不推荐。来源：[Choosing a Catalog Database](https://ducklake.select/docs/stable/duckdb/usage/choosing_a_catalog_database)。

DuckLake 对数据操作与 DDL 提供 ACID 和 snapshot isolation；一次提交对应一个快照。多个 writer 提交时可能在快照编号上冲突，无逻辑冲突的提交会自动重试，有逻辑冲突的提交会中止；并发 delete、ALTER、compaction 等依然存在冲突边界。来源：[Transactions](https://ducklake.select/docs/stable/duckdb/advanced_features/transactions)、[Conflict Resolution](https://ducklake.select/docs/stable/duckdb/advanced_features/conflict_resolution)。

**产品推论：** 平台要为任务定义业务幂等键、源端 checkpoint 和提交记录。在目标数据提交后、平台元数据状态落库前发生宕机，是两套系统之间的状态窗口；DuckLake 单次 ACID 提交不能自动实现源库 CDC 到分析湖的端到端 exactly-once。表无唯一约束时，还要用批次去重或确定性的 merge 规则保证重跑结果。

## 4. DuckLake 是湖仓格式，不是 Iceberg 的别名

DuckDB 是计算/数据库引擎；DuckLake 是使用事务型 SQL catalog 管理元数据、使用 Parquet 保存数据的开放湖仓格式；对象存储是文件存储层。三者的生命周期和故障域不同。来源：[DuckLake 1.0 Specification](https://ducklake.select/docs/stable/specification/introduction)。

Iceberg 使用 table metadata、manifest list、manifest 等格式跟踪快照及数据文件；DuckLake 把这些管理职责放入关系型 catalog。两者都可以涉及 Parquet，但格式及元数据协议不同。已有 DuckLake 到 Iceberg 导出的能力不能推导为同一格式或任意 engine 可直接互读写。来源：[Apache Iceberg Specification](https://iceberg.apache.org/spec/)、[DuckLake Manifesto](https://ducklake.select/manifesto/)、[DuckLake FAQ](https://ducklake.select/faq)。

**选择建议：** 如果当前主要由 DuckDB 读写、需要跨表事务及简单 catalog，可优先验证 DuckLake。如果客户现有多引擎数据湖明确要求 Iceberg，应依据该客户的 Spark/Trino/BI 写读矩阵验证 Iceberg 接入；不要为了“更先进”同时建设两个完整 lakehouse 路径。

### 4.1 Schema 演进、更新和业务键

DuckLake 支持添加、删除、重命名列/嵌套字段和表，支持无损类型提升，通常无需重写数据文件；并非任意类型变更都允许。上游字段把整数改成字符串、时区变更、精度缩小等应进入平台的数据契约/人工处理流程。来源：[Schema Evolution](https://ducklake.select/docs/stable/duckdb/usage/schema_evolution)。

DuckLake 不提供主键、外键、唯一性、CHECK 和索引等表级约束。源库中的主键不应在进入分析湖后被假定仍由引擎强制保证。来源：[DuckLake 扩展介绍](https://ducklake.select/docs/stable/duckdb/introduction)。

### 4.2 Compaction、快照与 GC

小批量写会带来小 Parquet 文件；数据 inlining 能缓解，之后仍要维护合并文件、重写大量删除的文件、快照过期和旧文件清理。删除表或旧快照不代表数据文件已经物理回收。来源：[Recommended Maintenance](https://ducklake.select/docs/stable/duckdb/maintenance/recommended_maintenance)。

1.0 提供 DuckLake `CHECKPOINT` 汇总上述维护函数，仍需配置 `expire_older_than`、`delete_older_than` 等并建立执行计划、观测和失败重试。名称相同不应与原生 DuckDB WAL checkpoint 混淆。来源：[DuckLake Checkpoint](https://ducklake.select/docs/stable/duckdb/maintenance/checkpoint)。

清理未引用/待删除文件要考虑仍在运行的长读事务。官方提供 dry-run；不能不加保留期就执行 `cleanup_all`。来源：[Cleanup of Files](https://ducklake.select/docs/stable/duckdb/maintenance/cleanup_of_files)。

**产品推论：** 一个可交付湖仓版本必须带维护面板或可观测后台任务，至少展示小文件数量、快照保留期、待清理字节、最长查询年龄和最近维护结果。否则“轻量”只是减少首次安装成本，后续运维成本被转移给用户。

### 4.3 HA 与备份的边界

DuckLake 官方分别说明 catalog 与文件存储的备份：PostgreSQL 可使用 dump 或 PITR，数据文件则依赖对象存储备份/版本化等机制；恢复旧 catalog 后，新提交即使仍有 Parquet 也不再被 catalog 跟踪，手动备份还要协调 compaction/cleanup。来源：[Backup and Recovery](https://ducklake.select/docs/stable/duckdb/guides/backups_and_recovery)。

**架构推论：** DuckLake 不自动提供整个产品的 HA。需要分别处理 PostgreSQL catalog、对象存储、平台元数据库、执行节点、任务租约与用户入口；应做真实恢复演练。多 writer、快照、对象存储副本、任务重试和高可用属于不同能力。不能用仅备份控制面的 PostgreSQL 或仅保留 Parquet 文件代替整个分析系统的恢复设计。

## 5. SQL 工作台与 AI 查询必须具备执行隔离

官方把来自不可信来源的 DuckDB SQL 当成 shell/Python 代码：它可能访问文件、网络、扩展和系统资源；设置只能提供防御的一部分，不能替代 sandbox。只读数据库也不等于禁止读取别的文件或发起网络访问。来源：[Securing DuckDB](https://duckdb.org/docs/current/operations_manual/securing_duckdb/overview)。

Quack 现在有认证与每条查询的授权 hook，所以「DuckDB 无任何认证/授权」也已不准确。但默认授权放行所有 SQL；需要 TLS reverse proxy。官方明确指出以 `SELECT`/`WITH` 前缀正则判断只读不可靠，并建议使用只读 attachment 或分析解析后的语句类型。来源：[Quack Security](https://duckdb.org/docs/current/quack/security)。

DuckLake 的权限需要结合 catalog 数据库和存储的权限实现；文档示例中的 Superuser/Writer/Reader 是指导性角色，不是 DuckLake 内建角色。表路径权限还依赖数据文件遵循路径约定。来源：[DuckLake Access Control](https://ducklake.select/docs/stable/duckdb/guides/access_control)。

**采用建议：** 首批客户按 workspace/客户隔离独立执行进程或容器、数据目录和服务凭据；应用权限与 worker 的最低权限一致。查询配额至少包括内存、spill 磁盘、线程、排队、超时/取消和结果大小。AI 生成 SQL 应走相同边界，用户不能通过多语句、CTE、表函数或路径访问绕过授权。不要把“前端禁止 DDL”作为执行安全证据。

## 6. 三个相近方案带来的竞争压力

| 方案 | 核实能力 | 对当前创业想法的影响 |
| --- | --- | --- |
| MotherDuck | serverless DuckDB 云平台；每用户/客户独立 Duckling，纵向放大实例和读副本横向扩容；client + cloud 双端执行 | “把 DuckDB 放到 server”“多用户”“查询副本”“AI SQL”已有产品，不能假定这个方向空白 |
| Rill | 本地 Developer、SQL 模型、YAML 指标语义层、dashboard；DuckDB 默认，支持其他 OLAP/云部署 | 客户会比较从数据到业务报表的完成时间；仅完成连接和 SQL 工作台还不足 |
| dbt + DuckDB | 现有 SQL/Python 转换、模型 DAG、测试、增量模型；dbt v2 已内置 DuckDB adapter，Fusion 引擎基于 Rust | SQL 转换编排、血缘与质量没有必要全部从零重写；Rust 也不是独有卖点 |

MotherDuck 官方 2026-07 的架构说明明确：云端每条查询使用单个高性能 DuckDB 节点，平台通过独立实例和读副本跨机器扩展；这是分布式平台能力，与集群内通用单 SQL MPP 不同。双端执行能够让 client/server 对同一查询的部分阶段协作。此处只是说明能力，本文不采用厂商比较基准的倍率、价格节省或数据规模宣传作为本项目性能承诺。来源：[MotherDuck Scaling Architecture](https://motherduck.com/learn/how-motherduck-scales-distributed-architecture/)、[MotherDuck Duckling Scaling](https://motherduck.com/blog/scaling-duckdb-with-ducklings/)、[Hybrid Execution 公告](https://motherduck.com/blog/announcing-motherduck-duckdb-in-the-cloud/)。

Rill 的当前开发文档已覆盖连接、模型、指标与 dashboard；本地 Developer 和团队托管 Cloud 的使用方式不同，不能仅依据开源仓库推断所有云能力都可一键私有部署。来源：[Rill 文档首页](https://docs.rilldata.com/)、[Rill 数据连接](https://docs.rilldata.com/developers/build/connectors)、[Rill Project 数据流程](https://docs.rilldata.com/developers/build/getting-started)。

dbt-duckdb 当前支持 append/delete+insert/merge/microbatch 等增量 table 模型，支持 DuckLake 配置；直接输出普通外部文件的 `external` materialization 仍有增量限制。dbt v2 在 2026-09 已内置 DuckDB adapter，官方当前列为 CLI only。不要把 adapter 可用推导为所有 dbt 服务功能、许可证和 DuckLake 操作都一致。来源：[dbt-duckdb 上游 README](https://github.com/duckdb/dbt-duckdb)、[DuckDB 对 dbt v2 的介绍](https://duckdb.org/2026/09/22/dbt-fusion)、[dbt 官方支持列表](https://docs.getdbt.com/docs/dbt/supported-features)。

**商业推论：** 当前更可验证的差异是特定行业/业务的数据接入、指标口径、低成本私有交付、可靠增量同步和客户支持。例如把某一类 ERP + 订单库 + Excel 合成可核对的经营报表。该例只是访谈假设，不是已经证实的客户需求。自建通用 BI、MCP、自然语言 SQL、全套 lakehouse、通用 MPP 和几十种连接器会使一人团队在找到客户前背上长期维护负担。

## 7. 当前资源下的采用次序

以下为工程/业务决策建议，不是承诺日期。

1. **先得到一个可验证业务场景。** 找到同类客户，取得真实样本和实际指标口径，记录现在每月花多少工时、谁会购买、为什么现有工具不够。首个可交付目标是一种数据流程和一组报表；能形成付费试点再扩展。
2. **单机默认、边界完整。** 锁定 DuckDB 版本，用一个受控 owner 完成导入、转换、只读查询、重跑和恢复。开始具备权限、备份、日志和资源配额。产品目标是客户能自己安装并得到可信结果。
3. **分布式独立作业。** 使用现有 scheduler，把不同同步任务/批次送到 worker。原生文件模式仍集中提交，不共享文件多 writer。采用 DuckLake 前先演练“worker 完成写入但状态回报失败”的恢复路径。
4. **确有并发瓶颈时用共享湖与查询节点。** DuckLake + PostgreSQL catalog + 对象存储，增加独立查询节点，路由整条 SQL，明确每次查询采用的快照。重点验证成本、延迟和可恢复性，避免先承诺无依据的 PB 能力。
5. **最后讨论单 SQL MPP。** 必须证明客户中的某些 SQL 经分区裁剪、预聚合、物化、调优和合理单机规格后仍达不到目标，并且客户愿意承担相关交付/运维成本。优先复用上游 SwanLake 的真正跨节点能力；只修复可回馈的小块能力，不一人长期维护大规模定制执行引擎。

数据尺寸本身不能决定阶段。测试需要同时记录数据格式、压缩率、查询扫描比例、join 基数/倾斜、同时查询数、内存、临时磁盘、对象存储请求和费用。DuckDB 支持超内存运算，但不同运算符与工作负载仍需要调优和测试；“100 GB”“1 TB”不是本项目已经验证的容量档位。来源：[DuckDB Workload Tuning](https://duckdb.org/docs/current/guides/performance/how_to_tune_workloads)。

## 8. 开源、授权与上游协作

DuckDB 与 DuckLake 扩展仓库使用 MIT；DuckLake 官方 FAQ 也明确规范与扩展均为 MIT。许可证文本允许修改、分发和销售副本，并要求保留版权与许可声明。来源：[DuckDB LICENSE](https://raw.githubusercontent.com/duckdb/duckdb/main/LICENSE)、[DuckLake LICENSE](https://raw.githubusercontent.com/duckdb/ducklake/main/LICENSE)、[DuckLake 许可说明](https://ducklake.select/faq)。

这不等于整个产品的所有依赖、品牌使用、商标、第三方连接器和商业部署已完成法律核查。商业交付前，应按锁定 commit/version 整理实际依赖及 license/NOTICE，选择项目许可证并核验发行与托管条款；本文不对未检查依赖作法律结论。

**协作建议：** 保持 upstream 默认分支的镜像；产品开发分支可以有，但每项修复/功能用短 feature 分支，尽量提交独立上游 PR。产品发布使用有验证的 tag 或 commit 与锁文件，不能让跟随社区 master 与生产自动升级混为一谈。共用 feature adapter 的机会大于为三个上游各维护一套长期业务 fork 的机会。

Rust 有助于构建执行服务和部署工具，但 DuckDB 引擎本身由 C++ 实现，Rust 是客户端/集成层选择。不能把 Rust portal 的内存安全或小体积直接推导成整套数据平台在所有环境更快、更安全；必须以交付和测试证明。来源：[Why DuckDB](https://duckdb.org/why_duckdb)、[DuckDB Rust Client](https://duckdb.org/docs/current/clients/rust/overview)。

## 9. 必须继续验证的未知

- **需求未知：** 当前没有客户，无法判断客户接受“数据中台”定位，还是只为经营报表、数据同步、ERP 对账等具体结果付费。也没有证据支持任何价格、市场规模或续费预测。
- **容量未知：** 项目当前实际 DuckDB、DuckLake、SwanLake 的版本与依赖组合能否复现上游能力；本报告没有执行吞吐/延迟/恢复基准。
- **CDC 正确性未知：** 源端 schema 漂移、offset/checkpoint、重放去重、删除、顺序与回补；catalog ACID 不能替代这些契约。
- **执行隔离未知：** worker 是否阻止任意路径/网络访问、是否分客户权限、超时是否真正取消、是否限制线程/内存/临时磁盘；API 登录不能单独证明这些。
- **分布式语义未知：** SwanLake 的多节点能力到底是路由、分区 batch、共享湖还是完整 SQL planner/shuffle；没有基准和故障用例前不能宣传已具备通用 MPP。
- **运维未知：** 对象存储/PG 故障、commit 后 worker 宕机、备份期间 compaction、长查询遇到 GC、控制面恢复后租约一致性等需要故障注入。
- **成本未知：** 同一服务器上部署所有组件是否真的比 dbt + DuckDB 或客户已有数据库更便宜；应把安装、升级、监控、支持工时算入总成本。
- **竞争未知：** 国内客户对 MotherDuck/Rill/Datay/现有 BI 的真实采用、数据留存偏好和支付理由；公开网页可证明存在产品，不能替代客户访谈。

建议的第一轮验证门槛（自行设定的退出/继续规则）：约 15–20 次同类客户访谈；至少 3 家允许接入真实业务数据的设计伙伴；至少 1 家为明确结果付款或书面接受付费试点。数字只是控制当前投入的实验标准，不是行业公认转化率。若访谈只有“技术很酷”而没有稳定业务痛点与购买人，应缩小场景或调整方向，不应继续增加基础设施范围。
