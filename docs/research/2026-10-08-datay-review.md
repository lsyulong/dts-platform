# DataY 竞品源码检视与对 DTS Platform 的启示

检视日期：2026-10-08（Asia/Shanghai）。对象：用户提供的 [DataY GitHub 仓库](https://github.com/yuyuejia/datay)、[产品网站](http://datay.yuyuejia.com.cn/) 及仓库中的官方帮助文档。代码基线为 `main` 分支提交 [`ccab0ed7896431ce0984faad5f53fbda1697b8d2`](https://github.com/yuyuejia/datay/commit/ccab0ed7896431ce0984faad5f53fbda1697b8d2)，提交日期 2026-10-05。

方法与限制：浏览第一方页面，并下载官方 codeload 源码快照进行只读静态审计；快照 README 与固定提交的 raw README 经换行规范化比较一致。没有启动或构建 DataY，没有连接其演示服务或执行源码中的测试。产品网站本次浏览访问失败，相关产品文档改从仓库的 `site/help` 检视。因此，以下区分“代码中已有实现”“本次未发现”和“运行时仍待验证”；没有用功能列表替代生产验证。

## 主要判断

DataY 证实了“DuckDB 作为中小企业轻量分析平台入口”已有另一套可运行实现思路，但它不能证明市场规模、付费意愿或自己的生产成熟度。它对 DTS Platform 的最大启示是业务闭环：集成数据、模型、统一指标、权限、问数形成连贯流程。Java/Rust 语言差异本身不足以构成商业壁垒。

用户原先认为 DataY 没有分布式能力，这一点需要修正：当前代码确实实现了 master/worker 分离、Redis 队列分发任务、worker 横向扩展，并支持 DuckLake 和 Quack 远程 DuckDB 连接。本次检视未发现把**一条 SQL** 分解成多个节点的子计划、进行跨节点数据交换和全局聚合的执行器。这应描述为“分布式任务执行 + 可接远程或共享湖仓”，不能直接称为“分布式 SQL 数仓”。证据见 [调度部署文档](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/site/help/guide/scheduler.md)、[RedisEventService](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/datay-web/src/main/java/com/data/datafusion/service/jobevent/RedisEventService.java)、[连接实现 DBUtils](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/datay-core/src/main/java/com/data/metadata/util/DBUtils.java#L116)。

对目前只有一位开发者、尚无客户的 DTS Platform，更合理的竞争方向是：把某一类客户从“业务库和 Excel”带到“可核对、可恢复、可分享的经营指标”，建立部署与数据正确性证据。过早以跨节点 SQL 能力对抗功能清单，会把主要精力花在最难且还未验证客户需求的部分。

## 能力与证据

| 维度 | 本次看到的实现 | 不能据此推定的能力 |
| --- | --- | --- |
| 轻量部署 | `standalone` 通过进程内事件运行；core 可独立执行配置；Web 使用 Java/Spring 与嵌入 DuckDB | 2 核 4G 下能承受任意数仓规模、并发 BI 或复杂 JOIN |
| 多节点任务 | master 调度、worker 消费 Redis 队列；共享元数据库；集群状态建议存到 MinIO | 单 SQL 多机并行、精确一次执行、worker 崩溃自动无损接管 |
| DuckDB/DuckLake | JDBC DuckDB；DuckLake 挂载与写入；`quack:` 远端连接 | 自动湖仓治理、跨节点 SQL 调度、经过验证的高可用湖仓 |
| 数据集成 | JDBC 全量与增量；MySQL binlog；PostgreSQL pgoutput；HTTP、文件、飞书多维表格、Doris Stream Load 等组件 | 所有连接器都通过故障恢复和类型兼容测试；CDC 达到 exactly-once |
| 业务语义 | 事实与维度模型、物化；原子与衍生指标；时间和共同维度查询 | 完整企业治理体系、所有指标形式和缓慢变化维度自动处理 |
| AI | 指标目录、描述、维度抽样、RAG、查询工具；MCP 管理工具 | 问数总能正确、任意 SQL 自动安全、业务定义无需人工确认 |
| 安全 | JWT、租户上下文、Hibernate 租户过滤、角色数据范围、指标权限相关测试 | 所有执行入口都存在不可绕过的租户隔离；任意 SQL 的物理资源隔离 |

表中轻量与集群部署证据见 [scheduler 文档](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/site/help/guide/scheduler.md) 与 [集群 Compose 示例](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/datay-web/src/main/docker/scheduler-cluster.yml)。集成证据见 [MySQLBinlogInput](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/datay-core/src/main/java/com/data/job/component/cdc/MySQLBinlogInput.java)、[PostgresCDCInput](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/datay-core/src/main/java/com/data/job/component/cdc/PostgresCDCInput.java) 及 [组件目录](https://github.com/yuyuejia/datay/tree/ccab0ed7896431ce0984faad5f53fbda1697b8d2/datay-core/src/main/java/com/data/job/component)。

业务语义证据见 [建模文档](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/site/help/guide/modeling.md)、[DataModelMaterializeService](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/datay-web/src/main/java/com/data/datafusion/service/DataModelMaterializeService.java)、[MetricService](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/datay-web/src/main/java/com/data/datafusion/service/MetricService.java#L284) 与 [MetricQueryService](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/datay-web/src/main/java/com/data/datafusion/service/MetricQueryService.java)。AI 证据见 [MetricQueryAssistant](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/datay-web/src/main/java/com/data/datafusion/ai/assistant/MetricQueryAssistant.java#L68)、[MetricQueryDataTool](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/datay-web/src/main/java/com/data/datafusion/ai/tool/MetricQueryDataTool.java) 与 [MCP 文档](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/datay-web/docs/datay-mcp-harness.md)。

## 值得认真学习的产品设计

1. **把语义层放在 AI 前面。** `MetricService` 对原子/衍生指标、公式引用和循环依赖有校验；问数助手先查指标定义和维度，再调用统一指标查询工具。这比“自然语言直接生成任意 SQL”更容易给经营分析建立稳定口径。DTS 应先形成“营业额到底含不含退款”“活跃客户按哪个时间字段计算”等版本化业务定义，然后在同一执行服务上开放 BI、API 和 AI。
2. **围绕数据可交付结果设计页面。** 模型物化、ETL 画布调试采样、指标查询是连续的操作对象。DTS 应用一个完整示例证明用户能导入订单、定义毛利、查看趋势并核对源系统，而不是展示若干后端引擎入口。
3. **保留单机和扩展路径。** DataY 单机避免 Redis 依赖；集群再增加 MySQL、Redis、可共享状态存储。DTS 若使用多个 Rust 服务，仍应以一个安装入口、统一配置、健康检查、备份命令和升级命令呈现给客户。服务数量最终会成为运维成本，而语言不会自动消除它。依据：[部署文档](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/site/help/guide/scheduler.md)、[ETL 采样实现](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/datay-core/src/main/java/com/data/job/FlowComponent.java#L118)。

## 可靠性边界：已有能力不等于生产承诺

以下是代码路径提示的风险，需要运行故障注入后才能定性。它们不代表对 DataY 全面安全审计，也不能证明其他路径没有补偿。

### 任务领取与主节点退位

`RedisEventService.takeStartJobEvent()` 使用 `startJobQueue.take()`；`StartJobEventHandler` 随后把任务放入本地执行池，用静态内存映射跟踪运行任务。本次在这条领取路径没有发现领取确认、可见性超时、运行租约和重新入队协议。若 worker 在取出后、完成前被强制终止，任务接管需要额外证明。依据：[RedisEventService](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/datay-web/src/main/java/com/data/datafusion/service/jobevent/RedisEventService.java#L165)、[StartJobEventHandler](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/datay-web/src/main/java/com/data/datafusion/service/jobevent/StartJobEventHandler.java#L66)。

`LeaderElection` 获锁后将 `isMaster` 设为 true，进入等待；检视路径只看到当选回调，没有看到丢失 Redis 锁后的退位回调和 fencing token。Redisson watchdog 并不能单独证明网络分区后旧 master 会停止调度。必须测试 Redis 断连、进程暂停超过锁租期、两个 master 同时恢复等场景。依据：[LeaderElection](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/datay-web/src/main/java/com/data/datafusion/service/cluster/LeaderElection.java#L113)。

### CDC 位点与目标提交

代码确有一个良好意图：`FlowFile` 携带来源位点，`FlowComponent` 在 sink 成功执行后保存状态，MySQL/PostgreSQL 从已保存位点启动。因此不应说“DataY 没有 checkpoint”。但 PostgreSQL 的 `handleCommit()` 在 `eventHandler.flush()` 后立即向源端设置 `AppliedLSN`、`FlushedLSN` 并反馈；而 `flush()` 最终调用的 `writeRecords()` 只是向下游内存队列 `put()`。源端确认时间早于目标 sink 持久化时间，这条链路在 worker 崩溃、槽清理或源端 WAL 回收时有恢复缺口需要核验。依据：[PostgresWalCollector](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/datay-core/src/main/java/com/data/job/component/cdc/PostgresWalCollector.java#L182)、[DefaultBinlogEventHandler](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/datay-core/src/main/java/com/data/job/component/cdc/DefaultBinlogEventHandler.java#L115)、[FlowComponent 写队列和状态保存](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/datay-core/src/main/java/com/data/job/FlowComponent.java#L58)。

更值得关注的是，`DefaultBinlogEventHandler` 为 CDC `FlowFile` 设置 `ERROR_IGNORE`，`FlowComponent` 看到该属性后会对执行异常 `continue`。如果一个批次写入失败，而后续成功批次保存更靠后的位点，失败批次可能被越过。本次没有看到这一默认忽略路径的死信队列和人工补偿闭环。上述结论是静态风险推断，尚未重现数据丢失。依据：[CDC 错误标记](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/datay-core/src/main/java/com/data/job/component/cdc/DefaultBinlogEventHandler.java#L160)、[异常忽略](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/datay-core/src/main/java/com/data/job/FlowComponent.java#L72)。

本次看到 CDC INSERT/UPDATE/DELETE 端到端测试，且有结果断言；这比只有 JSON 案例更扎实。但部分测试依赖本地数据库，前置条件不足时通过 `assumeTrue` 跳过。本次可见 `.github/workflows` 只有产品网站部署工作流，未发现这些测试在该 GitHub 仓库持续运行的流水线；项目也可能在其 CNB 主站运行 CI，尚未核实。不能因此宣称测试未运行。依据：[PostgreSQL 到 DuckLake 测试](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/datay-core/src/test/java/com/data/job/component/PostgresCdcToDuckLakeTest.java#L46)、[GitHub 工作流](https://github.com/yuyuejia/datay/tree/ccab0ed7896431ce0984faad5f53fbda1697b8d2/.github/workflows)。

### 租户、安全和资源控制

DataY 具备真实的租户机制：JWT claim 进入 `TenantContext`，请求结束清理上下文；后台任务也设置所属租户；Hibernate aspect 在无租户上下文时使用 dummy tenant 拦截。优点不能忽略。依据：[TenantFilter](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/datay-web/src/main/java/com/data/datafusion/security/TenantFilter.java)、[TenantFilterAspect](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/datay-web/src/main/java/com/data/datafusion/config/TenantFilterAspect.java)、[后台任务租户上下文](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/datay-web/src/main/java/com/data/datafusion/service/jobevent/StartJobEventHandler.java#L171)。

指标数据范围有一个应谨慎处理的默认值：事实表缺少受控维度时默认 `SKIP`，只告警并跳过该范围；可以改为 `DENY`。共享部署应验证这一语义是否符合预期权限。租户过滤开启失败的异常处理也继续执行，因此需要按调用边界验证隔离不能失效开放。依据：[MetricQueryService 默认策略](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/datay-web/src/main/java/com/data/datafusion/service/MetricQueryService.java#L84)、[缺维度处理](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/datay-web/src/main/java/com/data/datafusion/service/MetricQueryService.java#L619)、[tenant aspect 异常处理](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/datay-web/src/main/java/com/data/datafusion/config/TenantFilterAspect.java#L87)。

`DataSourceQueryService` 按 SQL 前缀区分查询和更新，在该共享执行器内本次未发现查询超时、最大行数、内存配额和 SQL 能力白名单；查询结果完整读入列表。外层入口可能限制权限和 SQL，需要逐入口进一步审计。MCP 管理写工具默认打开，有配置可关闭。对于 DTS 的多人平台和 AI 功能，控制面授权、数据权限、SQL 执行能力、对象存储授权、worker 资源限额必须形成一套可测试的边界。依据：[DataSourceQueryService](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/datay-web/src/main/java/com/data/datafusion/service/DataSourceQueryService.java#L62)、[MCP 默认配置](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/datay-web/src/main/resources/config/application.yml#L256)。

## DuckDB / DuckLake 宣传应如何解读

README 把 DuckLake 描述成 Iceberg 格式，这个表述不准确。DuckLake 有自己的开放湖仓格式规范和 SQL catalog；与 Iceberg 都可以使用 Parquet 数据文件不代表元数据格式相同。DTS 的对外文档应分别说明 DuckDB 原生文件、Parquet、DuckLake、Iceberg 的支持范围及互操作边界。依据：[DataY README](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/README.md)、[DuckLake 官方规范入口](https://ducklake.select/docs/stable/)。

还需避免使用已经过时的“DuckDB 永远只能单进程访问/写入”的绝对描述：2026-10-08 所见 DuckDB 官方当前文档说明，原生嵌入读写仍在一个 writer 进程内；Quack 远程协议为原生格式提供多进程客户端读写，文档注明在 1.5.2 时属 beta；DuckLake + PostgreSQL catalog 可让多个 DuckDB 实例并发读写，DuckLake v1.0 规范与实现面向生产。多进程读写并不自动构成一条 SQL 多机执行。DataY 当前源码已经包含 Quack 支持，core JDBC 依赖为 1.5.3.0。依据：[DuckDB 并发官方文档](https://duckdb.org/docs/current/connect/concurrency#multiple-processes)、[DataY Quack 连接](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/datay-core/src/main/java/com/data/metadata/util/DBUtils.java#L271)、[core pom](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/datay-core/pom.xml)。文档提到预期未来版本的成熟时间，不能据此认定 2.0 已发布或 Quack 已完成生产成熟验证。

## 开源、活跃度与商业判断

DataY 基线仓库顶层包含 Apache-2.0 LICENSE，许可证确实在 2026-10-05 提交加入。其第 2、4、6、9 节分别涉及使用/派生/分发、分发附带义务、商标、收费支持。若借鉴代码，应落实许可证和第三方依赖义务；不能把同一仓库许可证直接套到所有依赖。此处仅记录许可文本，不是完整商业法务结论。依据：[固定 LICENSE](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/LICENSE)。

本次 GitHub 页面显示 159 次提交、2 stars、1 fork，最近检视基线是 2026-10-05；近期历史中有功能和修复提交。GitHub Releases 页面本次未看到发布条目，父项目版本仍为 `1.0.0-SNAPSHOT`。这些证据说明它处于持续开发阶段，不能证明没有用户，也不能证明已经获得规模化市场验证。README 的默认 admin/admin、2 核 4G、单机性能等文字，应视为启动说明或作者宣称；本次未发现可复核的负载配置、P95 并发数据、灾备演练或付费客户数据。依据：[仓库首页](https://github.com/yuyuejia/datay)、[提交历史](https://github.com/yuyuejia/datay/commits/main)、[Releases](https://github.com/yuyuejia/datay/releases)、[父 pom](https://github.com/yuyuejia/datay/blob/ccab0ed7896431ce0984faad5f53fbda1697b8d2/pom.xml)。

因此，DataY 对 DTS 是有价值的产品参考和需要持续跟踪的竞争实现，但还不是能够直接宣判创业成功或失败的证据。也不应看到相似方向就认定“入口已被占满”。真正需要尽快确认的是：目标客户现在为哪些报表痛苦、源系统是什么、能否提供样本数据、愿为部署/运维/指标交付付多少、谁实际使用和签字验收。

建议把后续 6—8 周的验证集中在一个场景和三项可验收结果：

1. 用同一套真实或脱敏业务数据，在客户预期硬件上完成全量同步、增量和退款/删除处理，展示来源与汇总的核对结果。
2. 对“worker 强制终止、目标写入失败、位点提交前后崩溃、源表变更、备份恢复”给出明确可重复的测试和数据正确性结论。能解释恢复边界比声称 exactly-once 更有价值。
3. 让用户从导入数据到看到一个约定经营指标只走短流程，提供安装、升级和恢复说明；记录客户能否独立操作以及愿意付费的内容。

这些是本报告对 DTS 的建议，尚非已经发生的市场事实。对个人创业阶段而言，“有几位愿意共同试点的客户 + 一个可靠闭环”比“多个开源引擎 + 全套中台菜单 + 自研分布式 SQL”更能决定是否继续投入。
